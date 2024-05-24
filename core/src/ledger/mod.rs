mod cala;
mod config;
mod constants;
pub mod error;
pub mod fixed_term_loan;
mod tx_template;
pub mod user;

use tracing::instrument;

use crate::primitives::{
    FixedTermLoanId, LedgerAccountId, LedgerTxId, LedgerTxTemplateId, Satoshis, UsdCents,
};

use cala::*;
pub use config::*;
use error::*;
use fixed_term_loan::*;
use user::*;

#[derive(Clone)]
pub struct Ledger {
    pub cala: CalaClient,
}

impl Ledger {
    pub async fn init(config: LedgerConfig) -> Result<Self, LedgerError> {
        let cala = CalaClient::new(config.cala_url);
        Self::initialize_journal(&cala).await?;
        Self::initialize_global_accounts(&cala).await?;
        Self::initialize_tx_templates(&cala).await?;
        Ok(Ledger { cala })
    }

    #[instrument(name = "lava.ledger.get_balance", skip(self), err)]
    pub async fn get_balance(
        &self,
        account_ids: UserLedgerAccountIds,
    ) -> Result<UserBalance, LedgerError> {
        self.cala
            .get_user_balance(account_ids)
            .await?
            .ok_or(LedgerError::AccountNotFound)
    }

    #[instrument(
        name = "lava.ledger.create_unallocated_collateral_account_for_user",
        skip(self),
        err
    )]
    pub async fn create_accounts_for_user(
        &self,
        bitfinex_username: &str,
    ) -> Result<UserLedgerAccountIds, LedgerError> {
        let account_ids = UserLedgerAccountIds::new();
        Self::assert_account_exists(
            &self.cala,
            account_ids.unallocated_collateral_id,
            &format!("USERS.UNALLOCATED_COLLATERAL.{}", bitfinex_username),
            &format!("USERS.UNALLOCATED_COLLATERAL.{}", bitfinex_username),
            &format!("usr:bfx-{}:unallocated_collateral", bitfinex_username),
        )
        .await?;

        Self::assert_account_exists(
            &self.cala,
            account_ids.checking_id,
            &format!("USERS.CHECKING.{}", bitfinex_username),
            &format!("USERS.CHECKING.{}", bitfinex_username),
            &format!("usr:bfx-{}:checking", bitfinex_username),
        )
        .await?;

        Ok(account_ids)
    }

    #[instrument(
        name = "lava.ledger.create_unallocated_collateral_account_for_user",
        skip(self),
        err
    )]
    pub async fn create_accounts_for_loan(
        &self,
        loan_id: FixedTermLoanId,
        FixedTermLoanAccountIds {
            collateral_account_id,
            principal_account_id,
        }: FixedTermLoanAccountIds,
    ) -> Result<(), LedgerError> {
        Self::assert_account_exists(
            &self.cala,
            collateral_account_id,
            &format!("LOAN.COLLATERAL.{}", loan_id),
            &format!("LOAN.COLLATERAL.{}", loan_id),
            &format!("LOAN.COLLATERAL.{}", loan_id),
        )
        .await?;

        Self::assert_account_exists(
            &self.cala,
            principal_account_id,
            &format!("LOAN.PRINCIPAL.{}", loan_id),
            &format!("LOAN.PRINCIPAL.{}", loan_id),
            &format!("LOAN.PRINCIPAL.{}", loan_id),
        )
        .await?;

        Ok(())
    }

    pub async fn topup_collateral_for_user(
        &self,
        id: LedgerAccountId,
        amount: Satoshis,
        reference: String,
    ) -> Result<(), LedgerError> {
        Ok(self
            .cala
            .execute_topup_unallocated_collateral_tx(id, amount.to_btc(), reference)
            .await?)
    }

    pub async fn approve_loan(
        &self,
        tx_id: LedgerTxId,
        loan_account_ids: FixedTermLoanAccountIds,
        user_account_ids: UserLedgerAccountIds,
        collateral: Satoshis,
        principal: UsdCents,
        external_id: String,
    ) -> Result<(), LedgerError> {
        Ok(self
            .cala
            .execute_approve_loan_tx(
                tx_id,
                loan_account_ids,
                user_account_ids,
                collateral.to_btc(),
                principal.to_usd(),
                external_id,
            )
            .await?)
    }

    async fn initialize_journal(cala: &CalaClient) -> Result<(), LedgerError> {
        if cala
            .find_journal_by_id(constants::CORE_JOURNAL_ID)
            .await
            .is_ok()
        {
            return Ok(());
        }

        let err = match cala.create_core_journal(constants::CORE_JOURNAL_ID).await {
            Ok(_) => return Ok(()),
            Err(e) => e,
        };

        cala.find_journal_by_id(constants::CORE_JOURNAL_ID)
            .await
            .map_err(|_| err)?;
        Ok(())
    }

    async fn initialize_global_accounts(cala: &CalaClient) -> Result<(), LedgerError> {
        Self::assert_account_exists(
            cala,
            constants::CORE_ASSETS_ID.into(),
            constants::CORE_ASSETS_NAME,
            constants::CORE_ASSETS_CODE,
            &constants::CORE_ASSETS_ID.to_string(),
        )
        .await?;
        Ok(())
    }

    async fn assert_account_exists(
        cala: &CalaClient,
        account_id: LedgerAccountId,
        name: &str,
        code: &str,
        external_id: &str,
    ) -> Result<LedgerAccountId, LedgerError> {
        if let Ok(Some(id)) = cala
            .find_account_by_external_id(external_id.to_owned())
            .await
        {
            return Ok(id);
        }

        let err = match cala
            .create_account(
                account_id,
                name.to_owned(),
                code.to_owned(),
                external_id.to_owned(),
            )
            .await
        {
            Ok(id) => return Ok(id),
            Err(e) => e,
        };

        cala.find_account_by_external_id(external_id.to_owned())
            .await
            .map_err(|_| err)?
            .ok_or_else(|| LedgerError::CouldNotAssertAccountExits)
    }

    async fn initialize_tx_templates(cala: &CalaClient) -> Result<(), LedgerError> {
        Self::assert_topup_unallocated_collateral_tx_template_exists(
            cala,
            constants::TOPUP_UNALLOCATED_COLLATERAL_CODE,
        )
        .await?;
        Self::assert_approve_loan_tx_template_exists(cala, constants::APPROVE_LOAN_CODE).await?;
        Ok(())
    }

    async fn assert_topup_unallocated_collateral_tx_template_exists(
        cala: &CalaClient,
        template_code: &str,
    ) -> Result<LedgerTxTemplateId, LedgerError> {
        if let Ok(id) = cala
            .find_tx_template_by_code::<LedgerTxTemplateId>(template_code.to_owned())
            .await
        {
            return Ok(id);
        }

        let template_id = LedgerTxTemplateId::new();
        let err = match cala
            .create_topup_unallocated_collateral_tx_template(template_id)
            .await
        {
            Ok(id) => {
                return Ok(id);
            }
            Err(e) => e,
        };

        Ok(cala
            .find_tx_template_by_code::<LedgerTxTemplateId>(template_code.to_owned())
            .await
            .map_err(|_| err)?)
    }

    async fn assert_approve_loan_tx_template_exists(
        cala: &CalaClient,
        template_code: &str,
    ) -> Result<LedgerTxTemplateId, LedgerError> {
        if let Ok(id) = cala
            .find_tx_template_by_code::<LedgerTxTemplateId>(template_code.to_owned())
            .await
        {
            return Ok(id);
        }

        let template_id = LedgerTxTemplateId::new();
        let err = match cala.create_approve_loan_tx_template(template_id).await {
            Ok(id) => {
                return Ok(id);
            }
            Err(e) => e,
        };

        Ok(cala
            .find_tx_template_by_code::<LedgerTxTemplateId>(template_code.to_owned())
            .await
            .map_err(|_| err)?)
    }
}
