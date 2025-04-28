use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use audit::AuditInfo;

mod constants;
mod credit_facility_accounts;
pub mod error;
mod templates;
mod velocity;

use cala_ledger::{
    account::NewAccount,
    account_set::{AccountSet, AccountSetMemberId, AccountSetUpdate, NewAccountSet},
    velocity::{NewVelocityControl, VelocityControlId},
    CalaLedger, Currency, DebitOrCredit, JournalId, LedgerOperation, TransactionId,
};

use crate::{
    credit_facility::CreditFacilityBalanceSummary,
    payment_allocation::PaymentAllocation,
    primitives::{
        CalaAccountId, CalaAccountSetId, CollateralAction, CreditFacilityId, CustomerType,
        DisbursedReceivableAccountCategory, DisbursedReceivableAccountType,
        InterestReceivableAccountType, LedgerOmnibusAccountIds, LedgerTxId, Satoshis, UsdCents,
    },
    ChartOfAccountsIntegrationConfig, DurationType, Obligation, ObligationDueReallocationData,
    ObligationOverdueReallocationData,
};

use constants::*;
pub use credit_facility_accounts::*;
use error::*;

#[derive(Debug, Clone)]
pub struct CreditFacilityCollateralUpdate {
    pub tx_id: TransactionId,
    pub abs_diff: Satoshis,
    pub action: CollateralAction,
    pub credit_facility_account_ids: CreditFacilityAccountIds,
}

#[derive(Clone, Copy)]
pub struct InternalAccountSetDetails {
    id: CalaAccountSetId,
    normal_balance_type: DebitOrCredit,
}

#[derive(Clone, Copy)]
pub struct DisbursedReceivableAccountSets {
    individual: InternalAccountSetDetails,
    government_entity: InternalAccountSetDetails,
    private_company: InternalAccountSetDetails,
    bank: InternalAccountSetDetails,
    financial_institution: InternalAccountSetDetails,
    foreign_agency_or_subsidiary: InternalAccountSetDetails,
    non_domiciled_company: InternalAccountSetDetails,
}

impl DisbursedReceivableAccountSets {
    fn account_set_ids(&self) -> Vec<CalaAccountSetId> {
        vec![
            self.individual.id,
            self.government_entity.id,
            self.private_company.id,
            self.bank.id,
            self.financial_institution.id,
            self.foreign_agency_or_subsidiary.id,
            self.non_domiciled_company.id,
        ]
    }
}

#[derive(Clone, Copy)]
pub struct DisbursedReceivable {
    short_term: DisbursedReceivableAccountSets,
    long_term: DisbursedReceivableAccountSets,
    overdue: DisbursedReceivableAccountSets,
}

#[derive(Clone, Copy)]
pub struct InterestReceivableAccountSets {
    individual: InternalAccountSetDetails,
    government_entity: InternalAccountSetDetails,
    private_company: InternalAccountSetDetails,
    bank: InternalAccountSetDetails,
    financial_institution: InternalAccountSetDetails,
    foreign_agency_or_subsidiary: InternalAccountSetDetails,
    non_domiciled_company: InternalAccountSetDetails,
}

impl InterestReceivableAccountSets {
    fn account_set_ids(&self) -> Vec<CalaAccountSetId> {
        vec![
            self.individual.id,
            self.government_entity.id,
            self.private_company.id,
            self.bank.id,
            self.financial_institution.id,
            self.foreign_agency_or_subsidiary.id,
            self.non_domiciled_company.id,
        ]
    }
}

#[derive(Clone, Copy)]
pub struct InterestReceivable {
    short_term: InterestReceivableAccountSets,
    long_term: InterestReceivableAccountSets,
}

#[derive(Clone, Copy)]
pub struct CreditFacilityInternalAccountSets {
    pub facility: InternalAccountSetDetails,
    pub collateral: InternalAccountSetDetails,
    pub disbursed_receivable: DisbursedReceivable,
    pub disbursed_defaulted: InternalAccountSetDetails,
    pub interest_receivable: InterestReceivable,
    pub interest_defaulted: InternalAccountSetDetails,
    pub interest_income: InternalAccountSetDetails,
    pub fee_income: InternalAccountSetDetails,
}

impl CreditFacilityInternalAccountSets {
    fn account_set_ids(&self) -> Vec<CalaAccountSetId> {
        let Self {
            facility,
            collateral,
            interest_income,
            fee_income,

            disbursed_receivable:
                DisbursedReceivable {
                    short_term: disbursed_short_term,
                    long_term: disbursed_long_term,
                    overdue: disbursed_overdue,
                },
            disbursed_defaulted,
            interest_receivable:
                InterestReceivable {
                    short_term: interest_short_term,
                    long_term: interest_long_term,
                },
            interest_defaulted,
        } = self;

        let mut ids = vec![
            facility.id,
            collateral.id,
            interest_income.id,
            fee_income.id,
            disbursed_defaulted.id,
            interest_defaulted.id,
        ];
        ids.extend(
            disbursed_short_term
                .account_set_ids()
                .into_iter()
                .chain(disbursed_long_term.account_set_ids())
                .chain(disbursed_overdue.account_set_ids())
                .chain(interest_short_term.account_set_ids())
                .chain(interest_long_term.account_set_ids()),
        );

        ids
    }
}

#[derive(Clone)]
pub struct CreditLedger {
    cala: CalaLedger,
    journal_id: JournalId,
    facility_omnibus_account_ids: LedgerOmnibusAccountIds,
    collateral_omnibus_account_ids: LedgerOmnibusAccountIds,
    internal_account_sets: CreditFacilityInternalAccountSets,
    credit_facility_control_id: VelocityControlId,
    usd: Currency,
    btc: Currency,
}

impl CreditLedger {
    pub async fn init(cala: &CalaLedger, journal_id: JournalId) -> Result<Self, CreditLedgerError> {
        templates::AddCollateral::init(cala).await?;
        templates::ActivateCreditFacility::init(cala).await?;
        templates::RemoveCollateral::init(cala).await?;
        templates::RecordPaymentAllocation::init(cala).await?;
        templates::RecordObligationDueBalance::init(cala).await?;
        templates::RecordObligationOverdueBalance::init(cala).await?;
        templates::CreditFacilityAccrueInterest::init(cala).await?;
        templates::CreditFacilityPostAccruedInterest::init(cala).await?;
        templates::InitiateDisbursal::init(cala).await?;
        templates::CancelDisbursal::init(cala).await?;
        templates::ConfirmDisbursal::init(cala).await?;

        let collateral_omnibus_normal_balance_type = DebitOrCredit::Debit;
        let collateral_omnibus_account_ids = Self::find_or_create_omnibus_account(
            cala,
            journal_id,
            format!("{journal_id}:{CREDIT_COLLATERAL_OMNIBUS_ACCOUNT_SET_REF}"),
            format!("{journal_id}:{CREDIT_COLLATERAL_OMNIBUS_ACCOUNT_REF}"),
            CREDIT_COLLATERAL_OMNIBUS_ACCOUNT_SET_NAME.to_string(),
            collateral_omnibus_normal_balance_type,
        )
        .await?;

        let facility_omnibus_normal_balance_type = DebitOrCredit::Debit;
        let facility_omnibus_account_ids = Self::find_or_create_omnibus_account(
            cala,
            journal_id,
            format!("{journal_id}:{CREDIT_FACILITY_OMNIBUS_ACCOUNT_SET_REF}"),
            format!("{journal_id}:{CREDIT_FACILITY_OMNIBUS_ACCOUNT_REF}"),
            CREDIT_FACILITY_OMNIBUS_ACCOUNT_SET_NAME.to_string(),
            facility_omnibus_normal_balance_type,
        )
        .await?;

        let facility_normal_balance_type = DebitOrCredit::Credit;
        let facility_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{CREDIT_FACILITY_REMAINING_ACCOUNT_SET_REF}"),
            CREDIT_FACILITY_REMAINING_ACCOUNT_SET_NAME.to_string(),
            facility_normal_balance_type,
        )
        .await?;

        let collateral_normal_balance_type = DebitOrCredit::Credit;
        let collateral_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{CREDIT_COLLATERAL_ACCOUNT_SET_REF}"),
            CREDIT_COLLATERAL_ACCOUNT_SET_NAME.to_string(),
            collateral_normal_balance_type,
        )
        .await?;

        let disbursed_receivable_normal_balance_type = DebitOrCredit::Debit;
        let short_term_individual_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!(
                "{journal_id}:{SHORT_TERM_CREDIT_INDIVIDUAL_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"
            ),
                SHORT_TERM_CREDIT_INDIVIDUAL_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;
        let short_term_government_entity_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!(
                    "{journal_id}:{SHORT_TERM_CREDIT_GOVERNMENT_ENTITY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"
                ),
                SHORT_TERM_CREDIT_GOVERNMENT_ENTITY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME
                    .to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;
        let short_term_private_company_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{SHORT_TERM_CREDIT_PRIVATE_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            SHORT_TERM_CREDIT_PRIVATE_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let short_term_bank_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{SHORT_TERM_CREDIT_BANK_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            SHORT_TERM_CREDIT_BANK_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let short_term_financial_institution_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!("{journal_id}:{SHORT_TERM_CREDIT_FINANCIAL_INSTITUTION_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
                SHORT_TERM_CREDIT_FINANCIAL_INSTITUTION_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;
        let short_term_foreign_agency_or_subsidiary_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!("{journal_id}:{SHORT_TERM_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
                SHORT_TERM_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME
                    .to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;
        let short_term_non_domiciled_company_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!("{journal_id}:{SHORT_TERM_CREDIT_NON_DOMICILED_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
                SHORT_TERM_CREDIT_NON_DOMICILED_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;

        let long_term_individual_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_INDIVIDUAL_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_INDIVIDUAL_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let long_term_government_entity_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_GOVERNMENT_ENTITY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_GOVERNMENT_ENTITY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let long_term_private_company_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_PRIVATE_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_PRIVATE_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let long_term_bank_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_BANK_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_BANK_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let long_term_financial_institution_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_FINANCIAL_INSTITUTION_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_FINANCIAL_INSTITUTION_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let long_term_foreign_agency_or_subsidiary_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let long_term_non_domiciled_company_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_NON_DOMICILED_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_NON_DOMICILED_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;

        let overdue_individual_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!(
                    "{journal_id}:{OVERDUE_CREDIT_INDIVIDUAL_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"
                ),
                OVERDUE_CREDIT_INDIVIDUAL_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;
        let overdue_government_entity_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!(
                    "{journal_id}:{OVERDUE_CREDIT_GOVERNMENT_ENTITY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"
                ),
                OVERDUE_CREDIT_GOVERNMENT_ENTITY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME
                    .to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;
        let overdue_private_company_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{OVERDUE_CREDIT_PRIVATE_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            OVERDUE_CREDIT_PRIVATE_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let overdue_bank_disbursed_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{OVERDUE_CREDIT_BANK_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
            OVERDUE_CREDIT_BANK_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;
        let overdue_financial_institution_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!("{journal_id}:{OVERDUE_CREDIT_FINANCIAL_INSTITUTION_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
                OVERDUE_CREDIT_FINANCIAL_INSTITUTION_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;
        let overdue_foreign_agency_or_subsidiary_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!("{journal_id}:{OVERDUE_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
                OVERDUE_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME
                    .to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;
        let overdue_non_domiciled_company_disbursed_receivable_account_set_id =
            Self::find_or_create_account_set(
                cala,
                journal_id,
                format!("{journal_id}:{OVERDUE_CREDIT_NON_DOMICILED_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_REF}"),
                OVERDUE_CREDIT_NON_DOMICILED_COMPANY_DISBURSED_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
                disbursed_receivable_normal_balance_type,
            )
            .await?;

        let disbursed_defaulted_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{CREDIT_DISBURSED_DEFAULTED_ACCOUNT_SET_REF}"),
            CREDIT_DISBURSED_DEFAULTED_ACCOUNT_SET_NAME.to_string(),
            disbursed_receivable_normal_balance_type,
        )
        .await?;

        let interest_receivable_normal_balance_type = DebitOrCredit::Debit;

        let short_term_individual_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{SHORT_TERM_CREDIT_INDIVIDUAL_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            SHORT_TERM_CREDIT_INDIVIDUAL_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let short_term_government_entity_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{SHORT_TERM_CREDIT_GOVERNMENT_ENTITY_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            SHORT_TERM_CREDIT_GOVERNMENT_ENTITY_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let short_term_private_company_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{SHORT_TERM_CREDIT_PRIVATE_COMPANY_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            SHORT_TERM_CREDIT_PRIVATE_COMPANY_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let short_term_bank_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{SHORT_TERM_CREDIT_BANK_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            SHORT_TERM_CREDIT_BANK_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        )
        .await?;

        let short_term_financial_institution_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{SHORT_TERM_CREDIT_FINANCIAL_INSTITUTION_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            SHORT_TERM_CREDIT_FINANCIAL_INSTITUTION_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let short_term_foreign_agency_or_subsidiary_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{SHORT_TERM_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            SHORT_TERM_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let short_term_non_domiciled_company_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{SHORT_TERM_CREDIT_NON_DOMICILED_COMPANY_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            SHORT_TERM_CREDIT_NON_DOMICILED_COMPANY_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let long_term_individual_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_INDIVIDUAL_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_INDIVIDUAL_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let long_term_government_entity_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_GOVERNMENT_ENTITY_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_GOVERNMENT_ENTITY_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let long_term_private_company_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_PRIVATE_COMPANY_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_PRIVATE_COMPANY_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let long_term_bank_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_BANK_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_BANK_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        )
        .await?;

        let long_term_financial_institution_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_FINANCIAL_INSTITUTION_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_FINANCIAL_INSTITUTION_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let long_term_foreign_agency_or_subsidiary_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_FOREIGN_AGENCY_OR_SUBSIDIARY_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let long_term_non_domiciled_company_interest_receivable_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{LONG_TERM_CREDIT_NON_DOMICILED_COMPANY_INTEREST_RECEIVABLE_ACCOUNT_SET_REF}"),
            LONG_TERM_CREDIT_NON_DOMICILED_COMPANY_INTEREST_RECEIVABLE_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        ).await?;

        let interest_defaulted_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{CREDIT_INTEREST_DEFAULTED_ACCOUNT_SET_REF}"),
            CREDIT_INTEREST_DEFAULTED_ACCOUNT_SET_NAME.to_string(),
            interest_receivable_normal_balance_type,
        )
        .await?;

        let interest_income_normal_balance_type = DebitOrCredit::Credit;
        let interest_income_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{CREDIT_INTEREST_INCOME_ACCOUNT_SET_REF}"),
            CREDIT_INTEREST_INCOME_ACCOUNT_SET_NAME.to_string(),
            interest_income_normal_balance_type,
        )
        .await?;

        let fee_income_normal_balance_type = DebitOrCredit::Credit;
        let fee_income_account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            format!("{journal_id}:{CREDIT_FEE_INCOME_ACCOUNT_SET_REF}"),
            CREDIT_FEE_INCOME_ACCOUNT_SET_NAME.to_string(),
            fee_income_normal_balance_type,
        )
        .await?;

        let disbursed_receivable = DisbursedReceivable {
            short_term: DisbursedReceivableAccountSets {
                individual: InternalAccountSetDetails {
                    id: short_term_individual_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                government_entity: InternalAccountSetDetails {
                    id: short_term_government_entity_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                private_company: InternalAccountSetDetails {
                    id: short_term_private_company_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                bank: InternalAccountSetDetails {
                    id: short_term_bank_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                financial_institution: InternalAccountSetDetails {
                    id: short_term_financial_institution_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                foreign_agency_or_subsidiary: InternalAccountSetDetails {
                    id: short_term_foreign_agency_or_subsidiary_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                non_domiciled_company: InternalAccountSetDetails {
                    id: short_term_non_domiciled_company_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
            },
            long_term: DisbursedReceivableAccountSets {
                individual: InternalAccountSetDetails {
                    id: long_term_individual_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                government_entity: InternalAccountSetDetails {
                    id: long_term_government_entity_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                private_company: InternalAccountSetDetails {
                    id: long_term_private_company_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                bank: InternalAccountSetDetails {
                    id: long_term_bank_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                financial_institution: InternalAccountSetDetails {
                    id: long_term_financial_institution_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                foreign_agency_or_subsidiary: InternalAccountSetDetails {
                    id: long_term_foreign_agency_or_subsidiary_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                non_domiciled_company: InternalAccountSetDetails {
                    id: long_term_non_domiciled_company_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
            },
            overdue: DisbursedReceivableAccountSets {
                individual: InternalAccountSetDetails {
                    id: overdue_individual_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                government_entity: InternalAccountSetDetails {
                    id: overdue_government_entity_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                private_company: InternalAccountSetDetails {
                    id: overdue_private_company_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                bank: InternalAccountSetDetails {
                    id: overdue_bank_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                financial_institution: InternalAccountSetDetails {
                    id: overdue_financial_institution_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                foreign_agency_or_subsidiary: InternalAccountSetDetails {
                    id: overdue_foreign_agency_or_subsidiary_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
                non_domiciled_company: InternalAccountSetDetails {
                    id: overdue_non_domiciled_company_disbursed_receivable_account_set_id,
                    normal_balance_type: disbursed_receivable_normal_balance_type,
                },
            },
        };

        let interest_receivable = InterestReceivable {
            short_term: InterestReceivableAccountSets {
                individual: InternalAccountSetDetails {
                    id: short_term_individual_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                government_entity: InternalAccountSetDetails {
                    id: short_term_government_entity_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                private_company: InternalAccountSetDetails {
                    id: short_term_private_company_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                bank: InternalAccountSetDetails {
                    id: short_term_bank_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                financial_institution: InternalAccountSetDetails {
                    id: short_term_financial_institution_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                foreign_agency_or_subsidiary: InternalAccountSetDetails {
                    id: short_term_foreign_agency_or_subsidiary_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                non_domiciled_company: InternalAccountSetDetails {
                    id: short_term_non_domiciled_company_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
            },
            long_term: InterestReceivableAccountSets {
                individual: InternalAccountSetDetails {
                    id: long_term_individual_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                government_entity: InternalAccountSetDetails {
                    id: long_term_government_entity_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                private_company: InternalAccountSetDetails {
                    id: long_term_private_company_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                bank: InternalAccountSetDetails {
                    id: long_term_bank_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                financial_institution: InternalAccountSetDetails {
                    id: long_term_financial_institution_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                foreign_agency_or_subsidiary: InternalAccountSetDetails {
                    id: long_term_foreign_agency_or_subsidiary_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
                non_domiciled_company: InternalAccountSetDetails {
                    id: long_term_non_domiciled_company_interest_receivable_account_set_id,
                    normal_balance_type: interest_receivable_normal_balance_type,
                },
            },
        };

        let internal_account_sets = CreditFacilityInternalAccountSets {
            facility: InternalAccountSetDetails {
                id: facility_account_set_id,
                normal_balance_type: facility_normal_balance_type,
            },
            collateral: InternalAccountSetDetails {
                id: collateral_account_set_id,
                normal_balance_type: collateral_normal_balance_type,
            },
            disbursed_receivable,
            disbursed_defaulted: InternalAccountSetDetails {
                id: disbursed_defaulted_account_set_id,
                normal_balance_type: disbursed_receivable_normal_balance_type,
            },
            interest_receivable,
            interest_defaulted: InternalAccountSetDetails {
                id: interest_defaulted_account_set_id,
                normal_balance_type: disbursed_receivable_normal_balance_type,
            },
            interest_income: InternalAccountSetDetails {
                id: interest_income_account_set_id,
                normal_balance_type: interest_income_normal_balance_type,
            },
            fee_income: InternalAccountSetDetails {
                id: fee_income_account_set_id,
                normal_balance_type: fee_income_normal_balance_type,
            },
        };

        let disbursal_limit_id = velocity::DisbursalLimit::init(cala).await?;

        let credit_facility_control_id = Self::create_credit_facility_control(cala).await?;

        match cala
            .velocities()
            .add_limit_to_control(credit_facility_control_id, disbursal_limit_id)
            .await
        {
            Ok(_)
            | Err(cala_ledger::velocity::error::VelocityError::LimitAlreadyAddedToControl) => {}
            Err(e) => return Err(e.into()),
        }

        Ok(Self {
            cala: cala.clone(),
            journal_id,
            facility_omnibus_account_ids,
            collateral_omnibus_account_ids,
            internal_account_sets,
            credit_facility_control_id,
            usd: Currency::USD,
            btc: Currency::BTC,
        })
    }

    async fn find_or_create_account_set(
        cala: &CalaLedger,
        journal_id: JournalId,
        reference: String,
        name: String,
        normal_balance_type: DebitOrCredit,
    ) -> Result<CalaAccountSetId, CreditLedgerError> {
        match cala
            .account_sets()
            .find_by_external_id(reference.to_string())
            .await
        {
            Ok(account_set) if account_set.values().journal_id != journal_id => {
                return Err(CreditLedgerError::JournalIdMismatch)
            }
            Ok(account_set) => return Ok(account_set.id),
            Err(e) if e.was_not_found() => (),
            Err(e) => return Err(e.into()),
        };

        let id = CalaAccountSetId::new();
        let new_account_set = NewAccountSet::builder()
            .id(id)
            .journal_id(journal_id)
            .external_id(reference.to_string())
            .name(name.clone())
            .description(name)
            .normal_balance_type(normal_balance_type)
            .build()
            .expect("Could not build new account set");
        match cala.account_sets().create(new_account_set).await {
            Ok(set) => Ok(set.id),
            Err(cala_ledger::account_set::error::AccountSetError::ExternalIdAlreadyExists) => {
                Ok(cala.account_sets().find_by_external_id(reference).await?.id)
            }

            Err(e) => Err(e.into()),
        }
    }

    async fn find_or_create_omnibus_account(
        cala: &CalaLedger,
        journal_id: JournalId,
        account_set_reference: String,
        reference: String,
        name: String,
        normal_balance_type: DebitOrCredit,
    ) -> Result<LedgerOmnibusAccountIds, CreditLedgerError> {
        let account_set_id = Self::find_or_create_account_set(
            cala,
            journal_id,
            account_set_reference,
            name.to_string(),
            normal_balance_type,
        )
        .await?;

        let members = cala
            .account_sets()
            .list_members_by_created_at(account_set_id, Default::default())
            .await?
            .entities;
        if !members.is_empty() {
            match members[0].id {
                AccountSetMemberId::Account(id) => {
                    return Ok(LedgerOmnibusAccountIds {
                        account_set_id,
                        account_id: id,
                    })
                }
                AccountSetMemberId::AccountSet(_) => {
                    return Err(CreditLedgerError::NonAccountMemberFoundInAccountSet(
                        account_set_id.to_string(),
                    ))
                }
            }
        }

        let mut op = cala.begin_operation().await?;
        let id = CalaAccountId::new();
        let new_ledger_account = NewAccount::builder()
            .id(id)
            .external_id(reference.to_string())
            .name(name.clone())
            .description(name)
            .code(id.to_string())
            .normal_balance_type(normal_balance_type)
            .build()
            .expect("Could not build new account");

        let account_id = match cala
            .accounts()
            .create_in_op(&mut op, new_ledger_account)
            .await
        {
            Ok(account) => {
                cala.account_sets()
                    .add_member_in_op(&mut op, account_set_id, account.id)
                    .await?;

                op.commit().await?;
                id
            }
            Err(cala_ledger::account::error::AccountError::ExternalIdAlreadyExists) => {
                op.commit().await?;
                cala.accounts().find_by_external_id(reference).await?.id
            }
            Err(e) => return Err(e.into()),
        };

        Ok(LedgerOmnibusAccountIds {
            account_set_id,
            account_id,
        })
    }

    pub async fn get_credit_facility_balance(
        &self,
        CreditFacilityAccountIds {
            facility_account_id,
            collateral_account_id,
            disbursed_receivable_not_yet_due_account_id,
            disbursed_receivable_due_account_id,
            disbursed_receivable_overdue_account_id,
            disbursed_defaulted_account_id,
            interest_receivable_not_yet_due_account_id,
            interest_receivable_due_account_id,
            interest_receivable_overdue_account_id,
            interest_defaulted_account_id,

            fee_income_account_id: _,
            interest_income_account_id: _,
        }: CreditFacilityAccountIds,
    ) -> Result<CreditFacilityBalanceSummary, CreditLedgerError> {
        let facility_id = (self.journal_id, facility_account_id, self.usd);
        let collateral_id = (self.journal_id, collateral_account_id, self.btc);
        let disbursed_receivable_not_yet_due_id = (
            self.journal_id,
            disbursed_receivable_not_yet_due_account_id,
            self.usd,
        );
        let disbursed_receivable_due_id = (
            self.journal_id,
            disbursed_receivable_due_account_id,
            self.usd,
        );
        let disbursed_receivable_overdue_id = (
            self.journal_id,
            disbursed_receivable_overdue_account_id,
            self.usd,
        );
        let disbursed_defaulted_id = (self.journal_id, disbursed_defaulted_account_id, self.usd);
        let interest_receivable_not_yet_due_id = (
            self.journal_id,
            interest_receivable_not_yet_due_account_id,
            self.usd,
        );
        let interest_receivable_due_id = (
            self.journal_id,
            interest_receivable_due_account_id,
            self.usd,
        );
        let interest_receivable_overdue_id = (
            self.journal_id,
            interest_receivable_overdue_account_id,
            self.usd,
        );
        let interest_defaulted_id = (self.journal_id, interest_defaulted_account_id, self.usd);
        let balances = self
            .cala
            .balances()
            .find_all(&[
                facility_id,
                collateral_id,
                disbursed_receivable_not_yet_due_id,
                disbursed_receivable_due_id,
                disbursed_receivable_overdue_id,
                disbursed_defaulted_id,
                interest_receivable_not_yet_due_id,
                interest_receivable_due_id,
                interest_receivable_overdue_id,
                interest_defaulted_id,
            ])
            .await?;
        let facility = if let Some(b) = balances.get(&facility_id) {
            UsdCents::try_from_usd(b.settled())?
        } else {
            UsdCents::ZERO
        };
        let disbursed = if let Some(b) = balances.get(&disbursed_receivable_not_yet_due_id) {
            UsdCents::try_from_usd(b.details.settled.dr_balance)?
        } else {
            UsdCents::ZERO
        };
        let not_yet_due_disbursed_outstanding =
            if let Some(b) = balances.get(&disbursed_receivable_not_yet_due_id) {
                UsdCents::try_from_usd(b.settled())?
            } else {
                UsdCents::ZERO
            };
        let due_disbursed_outstanding = if let Some(b) = balances.get(&disbursed_receivable_due_id)
        {
            UsdCents::try_from_usd(b.settled())?
        } else {
            UsdCents::ZERO
        };
        let overdue_disbursed_outstanding =
            if let Some(b) = balances.get(&disbursed_receivable_overdue_id) {
                UsdCents::try_from_usd(b.settled())?
            } else {
                UsdCents::ZERO
            };
        let disbursed_defaulted = if let Some(b) = balances.get(&disbursed_defaulted_id) {
            UsdCents::try_from_usd(b.settled())?
        } else {
            UsdCents::ZERO
        };

        let interest_posted = if let Some(b) = balances.get(&interest_receivable_not_yet_due_id) {
            UsdCents::try_from_usd(b.details.settled.dr_balance)?
        } else {
            UsdCents::ZERO
        };
        let not_yet_due_interest_outstanding =
            if let Some(b) = balances.get(&interest_receivable_not_yet_due_id) {
                UsdCents::try_from_usd(b.settled())?
            } else {
                UsdCents::ZERO
            };
        let due_interest_outstanding = if let Some(b) = balances.get(&interest_receivable_due_id) {
            UsdCents::try_from_usd(b.settled())?
        } else {
            UsdCents::ZERO
        };
        let overdue_interest_outstanding =
            if let Some(b) = balances.get(&interest_receivable_overdue_id) {
                UsdCents::try_from_usd(b.settled())?
            } else {
                UsdCents::ZERO
            };
        let interest_defaulted = if let Some(b) = balances.get(&interest_defaulted_id) {
            UsdCents::try_from_usd(b.settled())?
        } else {
            UsdCents::ZERO
        };

        let collateral = if let Some(b) = balances.get(&collateral_id) {
            Satoshis::try_from_btc(b.settled())?
        } else {
            Satoshis::ZERO
        };
        Ok(CreditFacilityBalanceSummary {
            facility_remaining: facility,
            collateral,

            disbursed,
            interest_posted,

            not_yet_due_disbursed_outstanding,
            due_disbursed_outstanding,
            overdue_disbursed_outstanding,
            disbursed_defaulted,

            not_yet_due_interest_outstanding,
            due_interest_outstanding,
            overdue_interest_outstanding,
            interest_defaulted,
        })
    }

    pub async fn update_credit_facility_collateral(
        &self,
        op: es_entity::DbOp<'_>,
        CreditFacilityCollateralUpdate {
            tx_id,
            credit_facility_account_ids,
            abs_diff,
            action,
        }: CreditFacilityCollateralUpdate,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.ledger_operation_from_db_op(op);
        match action {
            CollateralAction::Add => {
                self.cala
                    .post_transaction_in_op(
                        &mut op,
                        tx_id,
                        templates::ADD_COLLATERAL_CODE,
                        templates::AddCollateralParams {
                            journal_id: self.journal_id,
                            currency: self.btc,
                            amount: abs_diff.to_btc(),
                            collateral_account_id: credit_facility_account_ids
                                .collateral_account_id,
                            bank_collateral_account_id: self
                                .collateral_omnibus_account_ids
                                .account_id,
                        },
                    )
                    .await
            }
            CollateralAction::Remove => {
                self.cala
                    .post_transaction_in_op(
                        &mut op,
                        tx_id,
                        templates::REMOVE_COLLATERAL_CODE,
                        templates::RemoveCollateralParams {
                            journal_id: self.journal_id,
                            currency: self.btc,
                            amount: abs_diff.to_btc(),
                            collateral_account_id: credit_facility_account_ids
                                .collateral_account_id,
                            bank_collateral_account_id: self
                                .collateral_omnibus_account_ids
                                .account_id,
                        },
                    )
                    .await
            }
        }?;
        op.commit().await?;
        Ok(())
    }

    async fn record_obligation_repayment_in_op(
        &self,
        op: &mut LedgerOperation<'_>,
        PaymentAllocation {
            id,
            obligation_id: tx_ref,
            amount,
            account_to_be_debited_id,
            receivable_account_id,
            ..
        }: PaymentAllocation,
    ) -> Result<(), CreditLedgerError> {
        let params = templates::RecordPaymentAllocationParams {
            journal_id: self.journal_id,
            currency: self.usd,
            amount: amount.to_usd(),
            receivable_account_id,
            account_to_be_debited_id,
            tx_ref: tx_ref.to_string(),
        };
        self.cala
            .post_transaction_in_op(
                op,
                id.into(),
                templates::RECORD_PAYMENT_ALLOCATION_CODE,
                params,
            )
            .await?;

        Ok(())
    }

    pub async fn record_obligation_repayments(
        &self,
        op: es_entity::DbOp<'_>,
        payments: Vec<PaymentAllocation>,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.ledger_operation_from_db_op(op);

        for payment in payments {
            self.record_obligation_repayment_in_op(&mut op, payment)
                .await?;
        }

        op.commit().await?;
        Ok(())
    }

    pub async fn record_obligation_due(
        &self,
        op: es_entity::DbOp<'_>,
        ObligationDueReallocationData {
            tx_id,
            amount,
            not_yet_due_account_id,
            due_account_id,
            ..
        }: ObligationDueReallocationData,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.ledger_operation_from_db_op(op);
        self.cala
            .post_transaction_in_op(
                &mut op,
                tx_id,
                templates::RECORD_OBLIGATION_DUE_BALANCE_CODE,
                templates::RecordObligationDueBalanceParams {
                    journal_id: self.journal_id,
                    amount: amount.to_usd(),
                    receivable_not_yet_due_account_id: not_yet_due_account_id,
                    receivable_due_account_id: due_account_id,
                },
            )
            .await?;
        op.commit().await?;
        Ok(())
    }

    pub async fn record_obligation_overdue(
        &self,
        op: es_entity::DbOp<'_>,
        ObligationOverdueReallocationData {
            tx_id,
            outstanding_amount,
            due_account_id,
            overdue_account_id,
            ..
        }: ObligationOverdueReallocationData,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.ledger_operation_from_db_op(op);
        self.cala
            .post_transaction_in_op(
                &mut op,
                tx_id,
                templates::RECORD_OBLIGATION_OVERDUE_BALANCE_CODE,
                templates::RecordObligationOverdueBalanceParams {
                    journal_id: self.journal_id,
                    amount: outstanding_amount.to_usd(),
                    receivable_due_account_id: due_account_id,
                    receivable_overdue_account_id: overdue_account_id,
                },
            )
            .await?;
        op.commit().await?;
        Ok(())
    }

    pub async fn complete_credit_facility(
        &self,
        op: es_entity::DbOp<'_>,
        CreditFacilityCompletion {
            tx_id,
            collateral,
            credit_facility_account_ids,
        }: CreditFacilityCompletion,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.ledger_operation_from_db_op(op);
        self.cala
            .post_transaction_in_op(
                &mut op,
                tx_id,
                templates::REMOVE_COLLATERAL_CODE,
                templates::RemoveCollateralParams {
                    journal_id: self.journal_id,
                    currency: self.btc,
                    amount: collateral.to_btc(),
                    collateral_account_id: credit_facility_account_ids.collateral_account_id,
                    bank_collateral_account_id: self.collateral_omnibus_account_ids.account_id,
                },
            )
            .await?;
        op.commit().await?;
        Ok(())
    }

    pub async fn activate_credit_facility(
        &self,
        op: es_entity::DbOp<'_>,
        CreditFacilityActivation {
            tx_id,
            tx_ref,
            credit_facility_account_ids,
            debit_account_id,
            facility_amount,
            structuring_fee_amount,
        }: CreditFacilityActivation,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.ledger_operation_from_db_op(op);
        self.cala
            .post_transaction_in_op(
                &mut op,
                tx_id,
                templates::ACTIVATE_CREDIT_FACILITY_CODE,
                templates::ActivateCreditFacilityParams {
                    journal_id: self.journal_id,
                    credit_omnibus_account: self.facility_omnibus_account_ids.account_id,
                    credit_facility_account: credit_facility_account_ids.facility_account_id,
                    facility_disbursed_receivable_account: credit_facility_account_ids
                        .disbursed_receivable_not_yet_due_account_id,
                    facility_fee_income_account: credit_facility_account_ids.fee_income_account_id,
                    debit_account_id,
                    facility_amount: facility_amount.to_usd(),
                    structuring_fee_amount: structuring_fee_amount.to_usd(),
                    currency: self.usd,
                    external_id: tx_ref,
                },
            )
            .await?;
        op.commit().await?;
        Ok(())
    }

    pub async fn record_interest_accrual(
        &self,
        op: es_entity::DbOp<'_>,
        CreditFacilityInterestAccrual {
            tx_id,
            tx_ref,
            interest,
            period,
            credit_facility_account_ids,
        }: CreditFacilityInterestAccrual,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.ledger_operation_from_db_op(op);
        self.cala
            .post_transaction_in_op(
                &mut op,
                tx_id,
                templates::CREDIT_FACILITY_ACCRUE_INTEREST_CODE,
                templates::CreditFacilityAccrueInterestParams {
                    journal_id: self.journal_id,

                    credit_facility_interest_receivable_account: credit_facility_account_ids
                        .interest_receivable_not_yet_due_account_id,
                    credit_facility_interest_income_account: credit_facility_account_ids
                        .interest_income_account_id,
                    interest_amount: interest.to_usd(),
                    external_id: tx_ref,
                    effective: period.end.date_naive(),
                },
            )
            .await?;
        op.commit().await?;
        Ok(())
    }

    pub async fn record_interest_accrual_cycle(
        &self,
        op: es_entity::DbOp<'_>,
        obligation: Obligation,
    ) -> Result<(), CreditLedgerError> {
        let interest_receivable_account_id =
            obligation.not_yet_due_accounts().receivable_account_id;
        let interest_income_account_id =
            obligation.not_yet_due_accounts().account_to_be_credited_id;
        let Obligation {
            tx_id,
            reference: tx_ref,
            initial_amount: interest,
            recorded_at: posted_at,
            ..
        } = obligation;

        let mut op = self.cala.ledger_operation_from_db_op(op);
        self.cala
            .post_transaction_in_op(
                &mut op,
                tx_id,
                templates::CREDIT_FACILITY_POST_ACCRUED_INTEREST_CODE,
                templates::CreditFacilityPostAccruedInterestParams {
                    journal_id: self.journal_id,

                    credit_facility_interest_receivable_account: interest_receivable_account_id,
                    credit_facility_interest_income_account: interest_income_account_id,
                    interest_amount: interest.to_usd(),
                    external_id: tx_ref,
                    effective: posted_at.date_naive(),
                },
            )
            .await?;
        op.commit().await?;
        Ok(())
    }

    pub async fn initiate_disbursal(
        &self,
        op: es_entity::DbOp<'_>,
        tx_id: impl Into<TransactionId>,
        amount: UsdCents,
        facility_account_id: CalaAccountId,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.ledger_operation_from_db_op(op);
        self.cala
            .post_transaction_in_op(
                &mut op,
                tx_id.into(),
                templates::INITIATE_DISBURSAL_CODE,
                templates::InitiateDisbursalParams {
                    journal_id: self.journal_id,
                    credit_omnibus_account: self.facility_omnibus_account_ids.account_id,
                    credit_facility_account: facility_account_id,
                    disbursed_amount: amount.to_usd(),
                },
            )
            .await?;
        op.commit().await?;
        Ok(())
    }

    pub async fn cancel_disbursal(
        &self,
        op: es_entity::DbOp<'_>,
        tx_id: LedgerTxId,
        amount: UsdCents,
        facility_account_id: CalaAccountId,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.ledger_operation_from_db_op(op);
        self.cala
            .post_transaction_in_op(
                &mut op,
                tx_id,
                templates::CANCEL_DISBURSAL_CODE,
                templates::CancelDisbursalParams {
                    journal_id: self.journal_id,
                    credit_omnibus_account: self.facility_omnibus_account_ids.account_id,
                    credit_facility_account: facility_account_id,
                    disbursed_amount: amount.to_usd(),
                },
            )
            .await?;
        op.commit().await?;
        Ok(())
    }

    pub async fn settle_disbursal(
        &self,
        op: es_entity::DbOp<'_>,
        obligation: Obligation,
        facility_account_id: CalaAccountId,
    ) -> Result<(), CreditLedgerError> {
        let facility_disbursed_receivable_account =
            obligation.not_yet_due_accounts().receivable_account_id;
        let account_to_be_credited_id = obligation.not_yet_due_accounts().account_to_be_credited_id;
        let Obligation {
            tx_id,
            reference: external_id,
            initial_amount: amount,
            ..
        } = obligation;

        let mut op = self.cala.ledger_operation_from_db_op(op);
        self.cala
            .post_transaction_in_op(
                &mut op,
                tx_id,
                templates::CONFIRM_DISBURSAL_CODE,
                templates::ConfirmDisbursalParams {
                    journal_id: self.journal_id,
                    credit_omnibus_account: self.facility_omnibus_account_ids.account_id,
                    credit_facility_account: facility_account_id,
                    facility_disbursed_receivable_account,
                    account_to_be_credited_id,
                    disbursed_amount: amount.to_usd(),
                    external_id,
                },
            )
            .await?;
        op.commit().await?;
        Ok(())
    }

    pub async fn create_credit_facility_control(
        cala: &CalaLedger,
    ) -> Result<VelocityControlId, CreditLedgerError> {
        let control = NewVelocityControl::builder()
            .id(CREDIT_FACILITY_VELOCITY_CONTROL_ID)
            .name("Credit Facility Control")
            .description("Velocity Control for Deposits")
            .build()
            .expect("build control");

        match cala.velocities().create_control(control).await {
            Err(cala_ledger::velocity::error::VelocityError::ControlIdAlreadyExists) => {
                Ok(CREDIT_FACILITY_VELOCITY_CONTROL_ID.into())
            }
            Err(e) => Err(e.into()),
            Ok(control) => Ok(control.id()),
        }
    }

    pub async fn add_credit_facility_control_to_account(
        &self,
        op: &mut cala_ledger::LedgerOperation<'_>,
        account_id: impl Into<CalaAccountId>,
    ) -> Result<(), CreditLedgerError> {
        self.cala
            .velocities()
            .attach_control_to_account_in_op(
                op,
                self.credit_facility_control_id,
                account_id.into(),
                cala_ledger::tx_template::Params::default(),
            )
            .await?;
        Ok(())
    }

    async fn create_account_in_op(
        &self,
        op: &mut LedgerOperation<'_>,
        id: impl Into<CalaAccountId>,
        parent_account_set: InternalAccountSetDetails,
        reference: &str,
        name: &str,
        description: &str,
    ) -> Result<(), CreditLedgerError> {
        let id = id.into();

        let new_ledger_account = NewAccount::builder()
            .id(id)
            .external_id(reference)
            .name(name)
            .description(description)
            .code(id.to_string())
            .normal_balance_type(parent_account_set.normal_balance_type)
            .build()
            .expect("Could not build new account");
        let ledger_account = self
            .cala
            .accounts()
            .create_in_op(op, new_ledger_account)
            .await?;
        self.cala
            .account_sets()
            .add_member_in_op(op, parent_account_set.id, ledger_account.id)
            .await?;

        Ok(())
    }

    fn disbursed_internal_account_set_from_type(
        &self,
        disbursed_account_type: impl Into<DisbursedReceivableAccountType>,
        disbursed_account_category: impl Into<DisbursedReceivableAccountCategory>,
    ) -> InternalAccountSetDetails {
        let disbursed_account_type = disbursed_account_type.into();
        let disbursed_account_category = disbursed_account_category.into();

        let term_type = match disbursed_account_category {
            DisbursedReceivableAccountCategory::ShortTerm => {
                &self.internal_account_sets.disbursed_receivable.short_term
            }
            DisbursedReceivableAccountCategory::LongTerm => {
                &self.internal_account_sets.disbursed_receivable.long_term
            }
            DisbursedReceivableAccountCategory::Overdue => {
                &self.internal_account_sets.disbursed_receivable.overdue
            }
        };

        match disbursed_account_type {
            DisbursedReceivableAccountType::Individual => term_type.individual,
            DisbursedReceivableAccountType::GovernmentEntity => term_type.government_entity,
            DisbursedReceivableAccountType::PrivateCompany => term_type.private_company,
            DisbursedReceivableAccountType::Bank => term_type.bank,
            DisbursedReceivableAccountType::FinancialInstitution => term_type.financial_institution,
            DisbursedReceivableAccountType::ForeignAgencyOrSubsidiary => {
                term_type.foreign_agency_or_subsidiary
            }
            DisbursedReceivableAccountType::NonDomiciledCompany => term_type.non_domiciled_company,
        }
    }

    // TODO: Consider adding separate 'overdue' account like in disbursed
    fn interest_internal_account_set_from_type(
        &self,
        interest_receivable_account_type: impl Into<InterestReceivableAccountType>,
        duration_type: DurationType,
    ) -> InternalAccountSetDetails {
        let interest_receivable_account_type = interest_receivable_account_type.into();

        let term_type = if duration_type == DurationType::ShortTerm {
            &self.internal_account_sets.interest_receivable.short_term
        } else {
            &self.internal_account_sets.interest_receivable.long_term
        };

        match interest_receivable_account_type {
            InterestReceivableAccountType::Individual => term_type.individual,
            InterestReceivableAccountType::GovernmentEntity => term_type.government_entity,
            InterestReceivableAccountType::PrivateCompany => term_type.private_company,
            InterestReceivableAccountType::Bank => term_type.bank,
            InterestReceivableAccountType::FinancialInstitution => term_type.financial_institution,
            InterestReceivableAccountType::ForeignAgencyOrSubsidiary => {
                term_type.foreign_agency_or_subsidiary
            }
            InterestReceivableAccountType::NonDomiciledCompany => term_type.non_domiciled_company,
        }
    }

    pub async fn create_accounts_for_credit_facility(
        &self,
        op: &mut cala_ledger::LedgerOperation<'_>,
        credit_facility_id: CreditFacilityId,
        account_ids: CreditFacilityAccountIds,
        customer_type: CustomerType,
        duration_type: DurationType,
    ) -> Result<(), CreditLedgerError> {
        let CreditFacilityAccountIds {
            facility_account_id,
            disbursed_receivable_not_yet_due_account_id,
            disbursed_receivable_due_account_id,
            disbursed_receivable_overdue_account_id,
            disbursed_defaulted_account_id,
            collateral_account_id,
            interest_receivable_not_yet_due_account_id,
            interest_receivable_due_account_id,
            interest_receivable_overdue_account_id,
            interest_defaulted_account_id,
            interest_income_account_id,
            fee_income_account_id,
        } = account_ids;

        let collateral_reference = &format!("credit-facility-collateral:{}", credit_facility_id);
        let collateral_name = &format!(
            "Credit Facility Collateral Account for {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            collateral_account_id,
            self.internal_account_sets.collateral,
            collateral_reference,
            collateral_name,
            collateral_name,
        )
        .await?;

        let facility_reference = &format!("credit-facility-obs-facility:{}", credit_facility_id);
        let facility_name = &format!(
            "Off-Balance-Sheet Facility Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            facility_account_id,
            self.internal_account_sets.facility,
            facility_reference,
            facility_name,
            facility_name,
        )
        .await?;

        let disbursed_receivable_not_yet_due_reference = &format!(
            "credit-facility-disbursed-not-yet-due-receivable:{}",
            credit_facility_id
        );
        let disbursed_receivable_not_yet_due_name = &format!(
            "Disbursed Receivable Not Yet Due Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            disbursed_receivable_not_yet_due_account_id,
            self.disbursed_internal_account_set_from_type(customer_type, duration_type),
            disbursed_receivable_not_yet_due_reference,
            disbursed_receivable_not_yet_due_name,
            disbursed_receivable_not_yet_due_name,
        )
        .await?;

        let disbursed_receivable_due_reference = &format!(
            "credit-facility-disbursed-due-receivable:{}",
            credit_facility_id
        );
        let disbursed_receivable_due_name = &format!(
            "Disbursed Receivable Due Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            disbursed_receivable_due_account_id,
            self.disbursed_internal_account_set_from_type(customer_type, duration_type),
            disbursed_receivable_due_reference,
            disbursed_receivable_due_name,
            disbursed_receivable_due_name,
        )
        .await?;

        let disbursed_receivable_overdue_reference = &format!(
            "credit-facility-disbursed-overdue-receivable:{}",
            credit_facility_id
        );
        let disbursed_receivable_overdue_name = &format!(
            "Disbursed Receivable Overdue Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            disbursed_receivable_overdue_account_id,
            self.disbursed_internal_account_set_from_type(
                customer_type,
                DisbursedReceivableAccountCategory::Overdue,
            ),
            disbursed_receivable_overdue_reference,
            disbursed_receivable_overdue_name,
            disbursed_receivable_overdue_name,
        )
        .await?;

        let disbursed_defaulted_reference =
            &format!("credit-facility-disbursed-defaulted:{}", credit_facility_id);
        let disbursed_defaulted_name = &format!(
            "Disbursed Defaulted Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            disbursed_defaulted_account_id,
            self.internal_account_sets.disbursed_defaulted,
            disbursed_defaulted_reference,
            disbursed_defaulted_name,
            disbursed_defaulted_name,
        )
        .await?;

        let interest_receivable_not_yet_due_reference = &format!(
            "credit-facility-interest-not-yet-due-receivable:{}",
            credit_facility_id
        );
        let interest_receivable_not_yet_due_name = &format!(
            "Interest Receivable Not Yet Due Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            interest_receivable_not_yet_due_account_id,
            self.interest_internal_account_set_from_type(customer_type, duration_type),
            interest_receivable_not_yet_due_reference,
            interest_receivable_not_yet_due_name,
            interest_receivable_not_yet_due_name,
        )
        .await?;

        let interest_receivable_due_reference = &format!(
            "credit-facility-interest-due-receivable:{}",
            credit_facility_id
        );
        let interest_receivable_due_name = &format!(
            "Interest Receivable Due Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            interest_receivable_due_account_id,
            self.interest_internal_account_set_from_type(customer_type, duration_type),
            interest_receivable_due_reference,
            interest_receivable_due_name,
            interest_receivable_due_name,
        )
        .await?;

        let interest_receivable_overdue_reference = &format!(
            "credit-facility-interest-overdue-receivable:{}",
            credit_facility_id
        );
        let interest_receivable_overdue_name = &format!(
            "Interest Receivable Overdue Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            interest_receivable_overdue_account_id,
            self.interest_internal_account_set_from_type(customer_type, duration_type),
            interest_receivable_overdue_reference,
            interest_receivable_overdue_name,
            interest_receivable_overdue_name,
        )
        .await?;

        let interest_defaulted_reference =
            &format!("credit-facility-interest-defaulted:{}", credit_facility_id);
        let interest_defaulted_name = &format!(
            "Interest Defaulted Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            interest_defaulted_account_id,
            self.internal_account_sets.interest_defaulted,
            interest_defaulted_reference,
            interest_defaulted_name,
            interest_defaulted_name,
        )
        .await?;

        let interest_income_reference =
            &format!("credit-facility-interest-income:{}", credit_facility_id);
        let interest_income_name = &format!(
            "Interest Income Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            interest_income_account_id,
            self.internal_account_sets.interest_income,
            interest_income_reference,
            interest_income_name,
            interest_income_name,
        )
        .await?;

        let fee_income_reference = &format!("credit-facility-fee-income:{}", credit_facility_id);
        let fee_income_name = &format!(
            "Fee Income Account for Credit Facility {}",
            credit_facility_id
        );
        self.create_account_in_op(
            op,
            fee_income_account_id,
            self.internal_account_sets.fee_income,
            fee_income_reference,
            fee_income_name,
            fee_income_name,
        )
        .await?;

        Ok(())
    }

    pub async fn get_chart_of_accounts_integration_config(
        &self,
    ) -> Result<Option<ChartOfAccountsIntegrationConfig>, CreditLedgerError> {
        let account_set_id = *self
            .internal_account_sets
            .account_set_ids()
            .first()
            .expect("No internal account set ids found");
        let account_set = self.cala.account_sets().find(account_set_id).await?;
        if let Some(meta) = account_set.values().metadata.as_ref() {
            let meta: ChartOfAccountsIntegrationMeta =
                serde_json::from_value(meta.clone()).expect("Could not deserialize metadata");
            Ok(Some(meta.config))
        } else {
            Ok(None)
        }
    }

    async fn attach_charts_account_set<F>(
        &self,
        op: &mut LedgerOperation<'_>,
        account_sets: &mut HashMap<CalaAccountSetId, AccountSet>,
        internal_account_set_id: CalaAccountSetId,
        parent_account_set_id: CalaAccountSetId,
        new_meta: &ChartOfAccountsIntegrationMeta,
        old_parent_id_getter: F,
    ) -> Result<(), CreditLedgerError>
    where
        F: FnOnce(ChartOfAccountsIntegrationMeta) -> CalaAccountSetId,
    {
        let mut internal_account_set = account_sets
            .remove(&internal_account_set_id)
            .expect("internal account set not found");

        if let Some(old_meta) = internal_account_set.values().metadata.as_ref() {
            let old_meta: ChartOfAccountsIntegrationMeta =
                serde_json::from_value(old_meta.clone()).expect("Could not deserialize metadata");
            let old_parent_account_set_id = old_parent_id_getter(old_meta);
            if old_parent_account_set_id != parent_account_set_id {
                self.cala
                    .account_sets()
                    .remove_member_in_op(op, old_parent_account_set_id, internal_account_set_id)
                    .await?;
            }
        }

        self.cala
            .account_sets()
            .add_member_in_op(op, parent_account_set_id, internal_account_set_id)
            .await?;
        let mut update = AccountSetUpdate::default();
        update
            .metadata(new_meta)
            .expect("Could not update metadata");
        internal_account_set.update(update);
        self.cala
            .account_sets()
            .persist_in_op(op, &mut internal_account_set)
            .await?;

        Ok(())
    }

    pub async fn attach_chart_of_accounts_account_sets(
        &self,
        charts_integration_meta: ChartOfAccountsIntegrationMeta,
    ) -> Result<(), CreditLedgerError> {
        let mut op = self.cala.begin_operation().await?;

        let mut account_set_ids = vec![
            self.facility_omnibus_account_ids.account_set_id,
            self.collateral_omnibus_account_ids.account_set_id,
        ];
        account_set_ids.extend(self.internal_account_sets.account_set_ids());
        let mut account_sets = self
            .cala
            .account_sets()
            .find_all_in_op::<AccountSet>(&mut op, &account_set_ids)
            .await?;

        let ChartOfAccountsIntegrationMeta {
            config: _,
            audit_info: _,

            facility_omnibus_parent_account_set_id,
            collateral_omnibus_parent_account_set_id,
            facility_parent_account_set_id,
            collateral_parent_account_set_id,
            interest_income_parent_account_set_id,
            fee_income_parent_account_set_id,
            short_term_disbursed_integration_meta,
            long_term_disbursed_integration_meta,
            short_term_interest_integration_meta,
            long_term_interest_integration_meta,
            overdue_disbursed_integration_meta,
        } = &charts_integration_meta;

        self.attach_charts_account_set(
            &mut op,
            &mut account_sets,
            self.facility_omnibus_account_ids.account_set_id,
            *facility_omnibus_parent_account_set_id,
            &charts_integration_meta,
            |meta| meta.facility_omnibus_parent_account_set_id,
        )
        .await?;

        self.attach_charts_account_set(
            &mut op,
            &mut account_sets,
            self.collateral_omnibus_account_ids.account_set_id,
            *collateral_omnibus_parent_account_set_id,
            &charts_integration_meta,
            |meta| meta.collateral_omnibus_parent_account_set_id,
        )
        .await?;

        self.attach_charts_account_set(
            &mut op,
            &mut account_sets,
            self.internal_account_sets.facility.id,
            *facility_parent_account_set_id,
            &charts_integration_meta,
            |meta| meta.facility_parent_account_set_id,
        )
        .await?;
        self.attach_charts_account_set(
            &mut op,
            &mut account_sets,
            self.internal_account_sets.collateral.id,
            *collateral_parent_account_set_id,
            &charts_integration_meta,
            |meta| meta.collateral_parent_account_set_id,
        )
        .await?;
        self.attach_charts_account_set(
            &mut op,
            &mut account_sets,
            self.internal_account_sets.interest_income.id,
            *interest_income_parent_account_set_id,
            &charts_integration_meta,
            |meta| meta.interest_income_parent_account_set_id,
        )
        .await?;
        self.attach_charts_account_set(
            &mut op,
            &mut account_sets,
            self.internal_account_sets.fee_income.id,
            *fee_income_parent_account_set_id,
            &charts_integration_meta,
            |meta| meta.fee_income_parent_account_set_id,
        )
        .await?;

        self.attach_short_term_disbursed_receivable_account_sets(
            &mut op,
            short_term_disbursed_integration_meta,
            &mut account_sets,
            &charts_integration_meta,
        )
        .await?;
        self.attach_long_term_disbursed_receivable_account_sets(
            &mut op,
            long_term_disbursed_integration_meta,
            &mut account_sets,
            &charts_integration_meta,
        )
        .await?;

        self.attach_short_term_interest_receivable_account_sets(
            &mut op,
            short_term_interest_integration_meta,
            &mut account_sets,
            &charts_integration_meta,
        )
        .await?;

        self.attach_long_term_interest_receivable_account_sets(
            &mut op,
            long_term_interest_integration_meta,
            &mut account_sets,
            &charts_integration_meta,
        )
        .await?;

        self.attach_overdue_disbursed_receivable_account_sets(
            &mut op,
            overdue_disbursed_integration_meta,
            &mut account_sets,
            &charts_integration_meta,
        )
        .await?;

        op.commit().await?;

        Ok(())
    }

    pub async fn attach_short_term_disbursed_receivable_account_sets(
        &self,
        op: &mut LedgerOperation<'_>,
        short_term_disbursed_integration_meta: &ShortTermDisbursedIntegrationMeta,
        account_sets: &mut HashMap<CalaAccountSetId, AccountSet>,
        charts_integration_meta: &ChartOfAccountsIntegrationMeta,
    ) -> Result<(), CreditLedgerError> {
        let short_term = &self.internal_account_sets.disbursed_receivable.short_term;

        let ShortTermDisbursedIntegrationMeta {
            short_term_individual_disbursed_receivable_parent_account_set_id,
            short_term_government_entity_disbursed_receivable_parent_account_set_id,
            short_term_private_company_disbursed_receivable_parent_account_set_id,
            short_term_bank_disbursed_receivable_parent_account_set_id,
            short_term_financial_institution_disbursed_receivable_parent_account_set_id,
            short_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id,
            short_term_non_domiciled_company_disbursed_receivable_parent_account_set_id,
        } = &short_term_disbursed_integration_meta;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.individual.id,
            *short_term_individual_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_disbursed_integration_meta
                    .short_term_individual_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.government_entity.id,
            *short_term_government_entity_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_disbursed_integration_meta
                    .short_term_government_entity_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.private_company.id,
            *short_term_private_company_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_disbursed_integration_meta
                    .short_term_private_company_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.bank.id,
            *short_term_bank_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_disbursed_integration_meta
                    .short_term_bank_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.financial_institution.id,
            *short_term_financial_institution_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_disbursed_integration_meta
                    .short_term_financial_institution_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.foreign_agency_or_subsidiary.id,

                *short_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| meta.short_term_disbursed_integration_meta.short_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id,
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.non_domiciled_company.id,
            *short_term_non_domiciled_company_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_disbursed_integration_meta
                    .short_term_non_domiciled_company_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        Ok(())
    }

    pub async fn attach_long_term_disbursed_receivable_account_sets(
        &self,
        op: &mut LedgerOperation<'_>,
        long_term_disbursed_integration_meta: &LongTermDisbursedIntegrationMeta,
        account_sets: &mut HashMap<CalaAccountSetId, AccountSet>,
        charts_integration_meta: &ChartOfAccountsIntegrationMeta,
    ) -> Result<(), CreditLedgerError> {
        let long_term = &self.internal_account_sets.disbursed_receivable.long_term;

        let LongTermDisbursedIntegrationMeta {
            long_term_individual_disbursed_receivable_parent_account_set_id,
            long_term_government_entity_disbursed_receivable_parent_account_set_id,
            long_term_private_company_disbursed_receivable_parent_account_set_id,
            long_term_bank_disbursed_receivable_parent_account_set_id,
            long_term_financial_institution_disbursed_receivable_parent_account_set_id,
            long_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id,
            long_term_non_domiciled_company_disbursed_receivable_parent_account_set_id,
        } = &long_term_disbursed_integration_meta;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.individual.id,
            *long_term_individual_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_disbursed_integration_meta
                    .long_term_individual_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.government_entity.id,
            *long_term_government_entity_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_disbursed_integration_meta
                    .long_term_government_entity_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.private_company.id,
            *long_term_private_company_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_disbursed_integration_meta
                    .long_term_private_company_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.bank.id,
            *long_term_bank_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_disbursed_integration_meta
                    .long_term_bank_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.financial_institution.id,
            *long_term_financial_institution_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_disbursed_integration_meta
                    .long_term_financial_institution_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.foreign_agency_or_subsidiary.id,
                *long_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| meta.long_term_disbursed_integration_meta.long_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id,
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.non_domiciled_company.id,
            *long_term_non_domiciled_company_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_disbursed_integration_meta
                    .long_term_non_domiciled_company_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        Ok(())
    }

    async fn attach_short_term_interest_receivable_account_sets(
        &self,
        op: &mut LedgerOperation<'_>,
        short_term_interest_integration_meta: &ShortTermInterestIntegrationMeta,
        account_sets: &mut HashMap<CalaAccountSetId, AccountSet>,
        charts_integration_meta: &ChartOfAccountsIntegrationMeta,
    ) -> Result<(), CreditLedgerError> {
        let short_term = &self.internal_account_sets.interest_receivable.short_term;

        let ShortTermInterestIntegrationMeta {
            short_term_individual_interest_receivable_parent_account_set_id,
            short_term_government_entity_interest_receivable_parent_account_set_id,
            short_term_private_company_interest_receivable_parent_account_set_id,
            short_term_bank_interest_receivable_parent_account_set_id,
            short_term_financial_institution_interest_receivable_parent_account_set_id,
            short_term_foreign_agency_or_subsidiary_interest_receivable_parent_account_set_id,
            short_term_non_domiciled_company_interest_receivable_parent_account_set_id,
        } = &short_term_interest_integration_meta;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.individual.id,
            *short_term_individual_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_interest_integration_meta
                    .short_term_individual_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.government_entity.id,
            *short_term_government_entity_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_interest_integration_meta
                    .short_term_government_entity_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.private_company.id,
            *short_term_private_company_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_interest_integration_meta
                    .short_term_private_company_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.bank.id,
            *short_term_bank_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_interest_integration_meta
                    .short_term_bank_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.financial_institution.id,
            *short_term_financial_institution_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_interest_integration_meta
                    .short_term_financial_institution_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.foreign_agency_or_subsidiary.id,
                *short_term_foreign_agency_or_subsidiary_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_interest_integration_meta
                    .short_term_foreign_agency_or_subsidiary_interest_receivable_parent_account_set_id
            },
        ).await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            short_term.non_domiciled_company.id,
            *short_term_non_domiciled_company_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.short_term_interest_integration_meta
                    .short_term_non_domiciled_company_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        Ok(())
    }

    async fn attach_long_term_interest_receivable_account_sets(
        &self,
        op: &mut LedgerOperation<'_>,
        long_term_interest_integration_meta: &LongTermInterestIntegrationMeta,
        account_sets: &mut HashMap<CalaAccountSetId, AccountSet>,
        charts_integration_meta: &ChartOfAccountsIntegrationMeta,
    ) -> Result<(), CreditLedgerError> {
        let long_term = &self.internal_account_sets.interest_receivable.long_term;

        let LongTermInterestIntegrationMeta {
            long_term_individual_interest_receivable_parent_account_set_id,
            long_term_government_entity_interest_receivable_parent_account_set_id,
            long_term_private_company_interest_receivable_parent_account_set_id,
            long_term_bank_interest_receivable_parent_account_set_id,
            long_term_financial_institution_interest_receivable_parent_account_set_id,
            long_term_foreign_agency_or_subsidiary_interest_receivable_parent_account_set_id,
            long_term_non_domiciled_company_interest_receivable_parent_account_set_id,
        } = &long_term_interest_integration_meta;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.individual.id,
            *long_term_individual_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_interest_integration_meta
                    .long_term_individual_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.government_entity.id,
            *long_term_government_entity_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_interest_integration_meta
                    .long_term_government_entity_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.private_company.id,
            *long_term_private_company_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_interest_integration_meta
                    .long_term_private_company_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.bank.id,
            *long_term_bank_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_interest_integration_meta
                    .long_term_bank_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.financial_institution.id,
            *long_term_financial_institution_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_interest_integration_meta
                    .long_term_financial_institution_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.foreign_agency_or_subsidiary.id,
                *long_term_foreign_agency_or_subsidiary_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_interest_integration_meta
                    .long_term_foreign_agency_or_subsidiary_interest_receivable_parent_account_set_id
            },
        ).await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            long_term.non_domiciled_company.id,
            *long_term_non_domiciled_company_interest_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.long_term_interest_integration_meta
                    .long_term_non_domiciled_company_interest_receivable_parent_account_set_id
            },
        )
        .await?;

        Ok(())
    }

    async fn attach_overdue_disbursed_receivable_account_sets(
        &self,
        op: &mut LedgerOperation<'_>,
        overdue_disbursed_integration_meta: &OverdueDisbursedIntegrationMeta,
        account_sets: &mut HashMap<CalaAccountSetId, AccountSet>,
        charts_integration_meta: &ChartOfAccountsIntegrationMeta,
    ) -> Result<(), CreditLedgerError> {
        let overdue = &self.internal_account_sets.disbursed_receivable.overdue;

        let OverdueDisbursedIntegrationMeta {
            overdue_individual_disbursed_receivable_parent_account_set_id,
            overdue_government_entity_disbursed_receivable_parent_account_set_id,
            overdue_private_company_disbursed_receivable_parent_account_set_id,
            overdue_bank_disbursed_receivable_parent_account_set_id,
            overdue_financial_institution_disbursed_receivable_parent_account_set_id,
            overdue_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id,
            overdue_non_domiciled_company_disbursed_receivable_parent_account_set_id,
        } = &overdue_disbursed_integration_meta;

        self.attach_charts_account_set(
            op,
            account_sets,
            overdue.individual.id,
            *overdue_individual_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.overdue_disbursed_integration_meta
                    .overdue_individual_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            overdue.government_entity.id,
            *overdue_government_entity_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.overdue_disbursed_integration_meta
                    .overdue_government_entity_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            overdue.private_company.id,
            *overdue_private_company_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.overdue_disbursed_integration_meta
                    .overdue_private_company_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            overdue.bank.id,
            *overdue_bank_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.overdue_disbursed_integration_meta
                    .overdue_bank_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            overdue.financial_institution.id,
            *overdue_financial_institution_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.overdue_disbursed_integration_meta
                    .overdue_financial_institution_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            overdue.foreign_agency_or_subsidiary.id,
            *overdue_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.overdue_disbursed_integration_meta
                    .overdue_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        self.attach_charts_account_set(
            op,
            account_sets,
            overdue.non_domiciled_company.id,
            *overdue_non_domiciled_company_disbursed_receivable_parent_account_set_id,
            charts_integration_meta,
            |meta| {
                meta.overdue_disbursed_integration_meta
                    .overdue_non_domiciled_company_disbursed_receivable_parent_account_set_id
            },
        )
        .await?;

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShortTermDisbursedIntegrationMeta {
    pub short_term_individual_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub short_term_government_entity_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub short_term_private_company_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub short_term_bank_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub short_term_financial_institution_disbursed_receivable_parent_account_set_id:
        CalaAccountSetId,
    pub short_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id:
        CalaAccountSetId,
    pub short_term_non_domiciled_company_disbursed_receivable_parent_account_set_id:
        CalaAccountSetId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LongTermDisbursedIntegrationMeta {
    pub long_term_individual_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub long_term_government_entity_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub long_term_private_company_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub long_term_bank_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub long_term_financial_institution_disbursed_receivable_parent_account_set_id:
        CalaAccountSetId,
    pub long_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id:
        CalaAccountSetId,
    pub long_term_non_domiciled_company_disbursed_receivable_parent_account_set_id:
        CalaAccountSetId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShortTermInterestIntegrationMeta {
    pub short_term_individual_interest_receivable_parent_account_set_id: CalaAccountSetId,
    pub short_term_government_entity_interest_receivable_parent_account_set_id: CalaAccountSetId,
    pub short_term_private_company_interest_receivable_parent_account_set_id: CalaAccountSetId,
    pub short_term_bank_interest_receivable_parent_account_set_id: CalaAccountSetId,
    pub short_term_financial_institution_interest_receivable_parent_account_set_id:
        CalaAccountSetId,
    pub short_term_foreign_agency_or_subsidiary_interest_receivable_parent_account_set_id:
        CalaAccountSetId,
    pub short_term_non_domiciled_company_interest_receivable_parent_account_set_id:
        CalaAccountSetId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LongTermInterestIntegrationMeta {
    pub long_term_individual_interest_receivable_parent_account_set_id: CalaAccountSetId,
    pub long_term_government_entity_interest_receivable_parent_account_set_id: CalaAccountSetId,
    pub long_term_private_company_interest_receivable_parent_account_set_id: CalaAccountSetId,
    pub long_term_bank_interest_receivable_parent_account_set_id: CalaAccountSetId,
    pub long_term_financial_institution_interest_receivable_parent_account_set_id: CalaAccountSetId,
    pub long_term_foreign_agency_or_subsidiary_interest_receivable_parent_account_set_id:
        CalaAccountSetId,
    pub long_term_non_domiciled_company_interest_receivable_parent_account_set_id: CalaAccountSetId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OverdueDisbursedIntegrationMeta {
    pub overdue_individual_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub overdue_government_entity_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub overdue_private_company_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub overdue_bank_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub overdue_financial_institution_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
    pub overdue_foreign_agency_or_subsidiary_disbursed_receivable_parent_account_set_id:
        CalaAccountSetId,
    pub overdue_non_domiciled_company_disbursed_receivable_parent_account_set_id: CalaAccountSetId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChartOfAccountsIntegrationMeta {
    pub config: ChartOfAccountsIntegrationConfig,
    pub audit_info: AuditInfo,

    pub facility_omnibus_parent_account_set_id: CalaAccountSetId,
    pub collateral_omnibus_parent_account_set_id: CalaAccountSetId,
    pub facility_parent_account_set_id: CalaAccountSetId,
    pub collateral_parent_account_set_id: CalaAccountSetId,
    pub interest_income_parent_account_set_id: CalaAccountSetId,
    pub fee_income_parent_account_set_id: CalaAccountSetId,

    pub short_term_disbursed_integration_meta: ShortTermDisbursedIntegrationMeta,
    pub long_term_disbursed_integration_meta: LongTermDisbursedIntegrationMeta,

    pub short_term_interest_integration_meta: ShortTermInterestIntegrationMeta,
    pub long_term_interest_integration_meta: LongTermInterestIntegrationMeta,

    pub overdue_disbursed_integration_meta: OverdueDisbursedIntegrationMeta,
}
