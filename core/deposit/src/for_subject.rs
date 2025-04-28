use audit::AuditSvc;
use authz::PermissionCheck;
use outbox::OutboxEventMarker;
use tracing::instrument;

use crate::{
    account::*,
    deposit::*,
    deposit_account_balance::*,
    deposit_account_cursor::DepositAccountsByCreatedAtCursor,
    error::*,
    event::*,
    history::{DepositAccountHistoryCursor, DepositAccountHistoryEntry},
    ledger::*,
    primitives::*,
    withdrawal::*,
};

pub struct DepositsForSubject<'a, Perms, E>
where
    Perms: PermissionCheck,
    E: OutboxEventMarker<CoreDepositEvent>,
{
    account_holder_id: DepositAccountHolderId,
    sub: &'a <<Perms as PermissionCheck>::Audit as AuditSvc>::Subject,
    accounts: &'a DepositAccountRepo<E>,
    deposits: &'a DepositRepo<E>,
    withdrawals: &'a WithdrawalRepo<E>,
    ledger: &'a DepositLedger,
    authz: &'a Perms,
}

impl<'a, Perms, E> DepositsForSubject<'a, Perms, E>
where
    Perms: PermissionCheck,
    <<Perms as PermissionCheck>::Audit as AuditSvc>::Action:
        From<CoreDepositAction> + From<GovernanceAction>,
    <<Perms as PermissionCheck>::Audit as AuditSvc>::Object:
        From<CoreDepositObject> + From<GovernanceObject>,
    E: OutboxEventMarker<CoreDepositEvent>,
{
    pub(super) fn new(
        subject: &'a <<Perms as PermissionCheck>::Audit as AuditSvc>::Subject,
        account_holder_id: DepositAccountHolderId,
        accounts: &'a DepositAccountRepo<E>,
        deposits: &'a DepositRepo<E>,
        withdrawals: &'a WithdrawalRepo<E>,
        ledger: &'a DepositLedger,
        authz: &'a Perms,
    ) -> Self {
        Self {
            sub: subject,
            account_holder_id,
            accounts,
            deposits,
            withdrawals,
            ledger,
            authz,
        }
    }

    pub async fn list_accounts_by_created_at(
        &self,
        query: es_entity::PaginatedQueryArgs<DepositAccountsByCreatedAtCursor>,
        direction: impl Into<es_entity::ListDirection> + std::fmt::Debug,
    ) -> Result<
        es_entity::PaginatedQueryRet<DepositAccount, DepositAccountsByCreatedAtCursor>,
        CoreDepositError,
    > {
        self.authz
            .audit()
            .record_entry(
                self.sub,
                CoreDepositObject::all_deposit_accounts(),
                CoreDepositAction::DEPOSIT_ACCOUNT_LIST,
                true,
            )
            .await?;

        Ok(self
            .accounts
            .list_for_account_holder_id_by_created_at(
                self.account_holder_id,
                query,
                direction.into(),
            )
            .await?)
    }

    #[instrument(name = "deposit.for_subject.account_balance", skip(self), err)]
    pub async fn account_balance(
        &self,
        account_id: impl Into<DepositAccountId> + std::fmt::Debug,
    ) -> Result<DepositAccountBalance, CoreDepositError> {
        let account_id = account_id.into();

        self.ensure_account_access(
            account_id,
            CoreDepositObject::deposit_account(account_id),
            CoreDepositAction::DEPOSIT_ACCOUNT_READ_BALANCE,
        )
        .await?;

        let balance = self.ledger.balance(account_id).await?;
        Ok(balance)
    }

    #[instrument(name = "deposit.for_subject.account_history", skip(self), err)]
    pub async fn account_history(
        &self,
        account_id: impl Into<DepositAccountId> + std::fmt::Debug,
        query: es_entity::PaginatedQueryArgs<DepositAccountHistoryCursor>,
    ) -> Result<
        es_entity::PaginatedQueryRet<DepositAccountHistoryEntry, DepositAccountHistoryCursor>,
        CoreDepositError,
    > {
        let account_id = account_id.into();

        self.ensure_account_access(
            account_id,
            CoreDepositObject::deposit_account(account_id),
            CoreDepositAction::DEPOSIT_ACCOUNT_READ,
        )
        .await?;

        let history = self
            .ledger
            .account_history::<DepositAccountHistoryEntry, DepositAccountHistoryCursor>(
                account_id, query,
            )
            .await?;
        Ok(history)
    }

    pub async fn list_deposits_for_account(
        &self,
        account_id: impl Into<DepositAccountId> + std::fmt::Debug,
    ) -> Result<Vec<Deposit>, CoreDepositError> {
        let account_id = account_id.into();

        self.ensure_account_access(
            account_id,
            CoreDepositObject::all_deposits(),
            CoreDepositAction::DEPOSIT_LIST,
        )
        .await?;

        Ok(self
            .deposits
            .list_for_deposit_account_id_by_created_at(
                account_id,
                Default::default(),
                es_entity::ListDirection::Descending,
            )
            .await?
            .entities)
    }

    pub async fn find_deposit_by_id(
        &self,
        deposit_id: impl Into<DepositId> + std::fmt::Debug,
    ) -> Result<Deposit, CoreDepositError> {
        let deposit_id = deposit_id.into();
        let deposit = self.deposits.find_by_id(deposit_id).await?;

        self.ensure_account_access(
            deposit.deposit_account_id,
            CoreDepositObject::deposit(deposit_id),
            CoreDepositAction::DEPOSIT_READ,
        )
        .await?;

        Ok(deposit)
    }

    pub async fn list_withdrawals_for_account(
        &self,
        account_id: impl Into<DepositAccountId> + std::fmt::Debug,
    ) -> Result<Vec<Withdrawal>, CoreDepositError> {
        let account_id = account_id.into();

        self.ensure_account_access(
            account_id,
            CoreDepositObject::all_withdrawals(),
            CoreDepositAction::WITHDRAWAL_LIST,
        )
        .await?;

        Ok(self
            .withdrawals
            .list_for_deposit_account_id_by_created_at(
                account_id,
                Default::default(),
                es_entity::ListDirection::Descending,
            )
            .await?
            .entities)
    }

    pub async fn find_withdrawal_by_id(
        &self,
        withdrawal_id: impl Into<WithdrawalId> + std::fmt::Debug,
    ) -> Result<Withdrawal, CoreDepositError> {
        let withdrawal_id = withdrawal_id.into();
        let withdrawal = self.withdrawals.find_by_id(withdrawal_id).await?;

        self.ensure_account_access(
            withdrawal.deposit_account_id,
            CoreDepositObject::withdrawal(withdrawal_id),
            CoreDepositAction::WITHDRAWAL_READ,
        )
        .await?;

        Ok(withdrawal)
    }

    pub async fn find_withdrawal_by_cancelled_tx_id(
        &self,
        cancelled_tx_id: impl Into<CalaTransactionId> + std::fmt::Debug,
    ) -> Result<Withdrawal, CoreDepositError> {
        let cancelled_tx_id = cancelled_tx_id.into();
        let withdrawal = self
            .withdrawals
            .find_by_cancelled_tx_id(Some(cancelled_tx_id))
            .await?;

        self.ensure_account_access(
            withdrawal.deposit_account_id,
            CoreDepositObject::withdrawal(withdrawal.id),
            CoreDepositAction::WITHDRAWAL_READ,
        )
        .await?;

        Ok(withdrawal)
    }

    async fn ensure_account_access(
        &self,
        account_id: DepositAccountId,
        object: CoreDepositObject,
        action: CoreDepositAction,
    ) -> Result<(), CoreDepositError> {
        let account = self.accounts.find_by_id(account_id).await?;

        if account.account_holder_id != self.account_holder_id {
            self.authz
                .audit()
                .record_entry(self.sub, object, action, false)
                .await?;
            return Err(CoreDepositError::DepositAccountNotFound);
        }
        self.authz
            .audit()
            .record_entry(self.sub, object, action, true)
            .await?;

        Ok(())
    }
}
