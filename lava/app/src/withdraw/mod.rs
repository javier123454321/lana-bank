mod approve_job;
mod entity;
pub mod error;
mod repo;

use authz::PermissionCheck;
use governance::ApprovalProcessType;

use crate::{
    audit::AuditInfo,
    authorization::{Authorization, Object, WithdrawAction},
    customer::Customers,
    data_export::Export,
    governance::Governance,
    job::Jobs,
    ledger::Ledger,
    outbox::Outbox,
    primitives::{CustomerId, Subject, UsdCents, WithdrawId},
};

pub use entity::*;
use error::WithdrawError;
pub use repo::{cursor::*, WithdrawRepo};

const APPROVE_WITHDRAW_PROCESS: ApprovalProcessType = ApprovalProcessType::new("withdraw");

#[derive(Clone)]
pub struct Withdraws {
    pool: sqlx::PgPool,
    repo: WithdrawRepo,
    customers: Customers,
    ledger: Ledger,
    authz: Authorization,
    governance: Governance,
    _jobs: Jobs,
}

impl Withdraws {
    #[allow(clippy::too_many_arguments)]
    pub async fn init(
        pool: &sqlx::PgPool,
        customers: &Customers,
        ledger: &Ledger,
        authz: &Authorization,
        export: &Export,
        governance: &Governance,
        jobs: &Jobs,
        outbox: &Outbox,
    ) -> Result<Self, WithdrawError> {
        let repo = WithdrawRepo::new(pool, export);
        jobs.add_initializer_and_spawn_unique(
            approve_job::WithdrawApprovalJobInitializer::new(pool, &repo, authz.audit(), outbox),
            approve_job::WithdrawApprovalJobConfig,
        )
        .await?;

        match governance.init_policy(APPROVE_WITHDRAW_PROCESS).await {
            Err(governance::error::GovernanceError::PolicyError(
                governance::policy_error::PolicyError::DuplicateApprovalProcessType,
            )) => (),
            Err(e) => return Err(e.into()),
            _ => (),
        }

        Ok(Self {
            pool: pool.clone(),
            repo,
            customers: customers.clone(),
            ledger: ledger.clone(),
            authz: authz.clone(),
            governance: governance.clone(),
            _jobs: jobs.clone(),
        })
    }

    pub fn repo(&self) -> &WithdrawRepo {
        &self.repo
    }

    pub async fn user_can_initiate(
        &self,
        sub: &Subject,
        enforce: bool,
    ) -> Result<Option<AuditInfo>, WithdrawError> {
        Ok(self
            .authz
            .evaluate_permission(sub, Object::Withdraw, WithdrawAction::Initiate, enforce)
            .await?)
    }

    pub async fn initiate(
        &self,
        sub: &Subject,
        customer_id: impl Into<CustomerId> + std::fmt::Debug,
        amount: UsdCents,
        reference: Option<String>,
    ) -> Result<Withdraw, WithdrawError> {
        let audit_info = self
            .user_can_initiate(sub, true)
            .await?
            .expect("audit info missing");
        let customer_id = customer_id.into();
        let customer = self.customers.repo().find_by_id(customer_id).await?;
        let id = WithdrawId::new();
        let new_withdraw = NewWithdraw::builder()
            .id(id)
            .approval_process_id(id)
            .customer_id(customer_id)
            .amount(amount)
            .reference(reference)
            .debit_account_id(customer.account_ids.on_balance_sheet_deposit_account_id)
            .audit_info(audit_info)
            .build()
            .expect("Could not build Withdraw");

        let mut db_tx = self.pool.begin().await?;
        self.governance
            .start_process(&mut db_tx, id, APPROVE_WITHDRAW_PROCESS)
            .await?;
        let withdraw = self.repo.create_in_tx(&mut db_tx, new_withdraw).await?;

        let customer_balances = self
            .ledger
            .get_customer_balance(customer.account_ids)
            .await?;
        if customer_balances.usd_balance.settled < amount {
            return Err(WithdrawError::InsufficientBalance(
                amount,
                customer_balances.usd_balance.settled,
            ));
        }

        self.ledger
            .initiate_withdrawal_for_customer(
                withdraw.id,
                customer.account_ids,
                withdraw.amount,
                format!("lava:withdraw:{}:initiate", withdraw.id),
            )
            .await?;

        db_tx.commit().await?;

        Ok(withdraw)
    }

    pub async fn user_can_confirm(
        &self,
        sub: &Subject,
        enforce: bool,
    ) -> Result<Option<AuditInfo>, WithdrawError> {
        Ok(self
            .authz
            .evaluate_permission(sub, Object::Withdraw, WithdrawAction::Confirm, enforce)
            .await?)
    }

    pub async fn confirm(
        &self,
        sub: &Subject,
        withdrawal_id: impl Into<WithdrawId> + std::fmt::Debug,
    ) -> Result<Withdraw, WithdrawError> {
        let audit_info = self
            .user_can_confirm(sub, true)
            .await?
            .expect("audit info missing");
        let id = withdrawal_id.into();
        let mut withdrawal = self.repo.find_by_id(id).await?;
        let tx_id = withdrawal.confirm(audit_info)?;

        let mut db_tx = self.pool.begin().await?;
        self.repo.update_in_tx(&mut db_tx, &mut withdrawal).await?;

        self.ledger
            .confirm_withdrawal_for_customer(
                tx_id,
                withdrawal.id,
                withdrawal.debit_account_id,
                withdrawal.amount,
                format!("lava:withdraw:{}:confirm", withdrawal.id),
            )
            .await?;

        db_tx.commit().await?;

        Ok(withdrawal)
    }

    pub async fn user_can_cancel(
        &self,
        sub: &Subject,
        enforce: bool,
    ) -> Result<Option<AuditInfo>, WithdrawError> {
        Ok(self
            .authz
            .evaluate_permission(sub, Object::Withdraw, WithdrawAction::Cancel, enforce)
            .await?)
    }

    pub async fn cancel(
        &self,
        sub: &Subject,
        withdrawal_id: impl Into<WithdrawId> + std::fmt::Debug,
    ) -> Result<Withdraw, WithdrawError> {
        let audit_info = self
            .user_can_cancel(sub, true)
            .await?
            .expect("audit info missing");

        let id = withdrawal_id.into();
        let mut withdrawal = self.repo.find_by_id(id).await?;
        let tx_id = withdrawal.cancel(audit_info)?;

        let mut db_tx = self.pool.begin().await?;
        self.repo.update_in_tx(&mut db_tx, &mut withdrawal).await?;

        self.ledger
            .cancel_withdrawal_for_customer(
                tx_id,
                withdrawal.id,
                withdrawal.debit_account_id,
                withdrawal.amount,
                format!("lava:withdraw:{}:cancel", withdrawal.id),
            )
            .await?;

        db_tx.commit().await?;

        Ok(withdrawal)
    }

    pub async fn find_by_id(
        &self,
        sub: &Subject,
        id: impl Into<WithdrawId> + std::fmt::Debug,
    ) -> Result<Option<Withdraw>, WithdrawError> {
        self.authz
            .enforce_permission(sub, Object::Withdraw, WithdrawAction::Read)
            .await?;

        match self.repo.find_by_id(id.into()).await {
            Ok(withdrawal) => Ok(Some(withdrawal)),
            Err(WithdrawError::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn list_for_customer(
        &self,
        sub: &Subject,
        customer_id: CustomerId,
    ) -> Result<Vec<Withdraw>, WithdrawError> {
        self.authz
            .enforce_permission(sub, Object::Withdraw, WithdrawAction::List)
            .await?;

        Ok(self
            .repo
            .list_for_customer_id_by_created_at(
                customer_id,
                Default::default(),
                es_entity::ListDirection::Descending,
            )
            .await?
            .entities)
    }

    pub async fn list(
        &self,
        sub: &Subject,
        query: es_entity::PaginatedQueryArgs<WithdrawByCreatedAtCursor>,
    ) -> Result<es_entity::PaginatedQueryRet<Withdraw, WithdrawByCreatedAtCursor>, WithdrawError>
    {
        self.authz
            .enforce_permission(sub, Object::Withdraw, WithdrawAction::List)
            .await?;
        self.repo
            .list_by_created_at(query, es_entity::ListDirection::Descending)
            .await
    }
}
