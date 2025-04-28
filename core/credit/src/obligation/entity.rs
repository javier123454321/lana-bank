use chrono::{DateTime, Utc};
use derive_builder::Builder;
use serde::{Deserialize, Serialize};

use audit::AuditInfo;
use es_entity::*;

use crate::{primitives::*, CreditFacilityId};

use super::error::ObligationError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObligationStatus {
    NotYetDue,
    Due,
    Overdue,
    Defaulted,
    Paid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ObligationAccounts {
    pub receivable_account_id: CalaAccountId,
    pub account_to_be_credited_id: CalaAccountId,
}

pub struct ObligationDueReallocationData {
    pub tx_id: LedgerTxId,
    pub amount: UsdCents,
    pub not_yet_due_account_id: CalaAccountId,
    pub due_account_id: CalaAccountId,
}

pub struct ObligationOverdueReallocationData {
    pub tx_id: LedgerTxId,
    pub outstanding_amount: UsdCents,
    pub due_account_id: CalaAccountId,
    pub overdue_account_id: CalaAccountId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ObligationsAmounts {
    pub disbursed: UsdCents,
    pub interest: UsdCents,
}

impl std::ops::Add<ObligationsAmounts> for ObligationsAmounts {
    type Output = Self;

    fn add(self, other: ObligationsAmounts) -> Self {
        Self {
            disbursed: self.disbursed + other.disbursed,
            interest: self.interest + other.interest,
        }
    }
}

impl ObligationsAmounts {
    pub const ZERO: Self = Self {
        disbursed: UsdCents::ZERO,
        interest: UsdCents::ZERO,
    };

    pub fn total(&self) -> UsdCents {
        self.interest + self.disbursed
    }

    pub fn is_zero(&self) -> bool {
        self.disbursed.is_zero() && self.interest.is_zero()
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(EsEvent, Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[es_event(id = "ObligationId")]
pub enum ObligationEvent {
    Initialized {
        id: ObligationId,
        credit_facility_id: CreditFacilityId,
        obligation_type: ObligationType,
        amount: UsdCents,
        reference: String,
        tx_id: LedgerTxId,
        not_yet_due_accounts: ObligationAccounts,
        due_accounts: ObligationAccounts,
        overdue_accounts: ObligationAccounts,
        due_date: DateTime<Utc>,
        overdue_date: Option<DateTime<Utc>>,
        defaulted_date: Option<DateTime<Utc>>,
        recorded_at: DateTime<Utc>,
        audit_info: AuditInfo,
    },
    DueRecorded {
        tx_id: LedgerTxId,
        audit_info: AuditInfo,
    },
    OverdueRecorded {
        tx_id: LedgerTxId,
        audit_info: AuditInfo,
    },
    Completed {
        completed_at: DateTime<Utc>,
        audit_info: AuditInfo,
    },
}

#[derive(EsEntity, Builder)]
#[builder(pattern = "owned", build_fn(error = "EsEntityError"))]
pub struct Obligation {
    pub id: ObligationId,
    pub tx_id: LedgerTxId,
    pub credit_facility_id: CreditFacilityId,
    pub reference: String,
    pub initial_amount: UsdCents,
    pub recorded_at: DateTime<Utc>,
    pub(super) events: EntityEvents<ObligationEvent>,
}

impl Obligation {
    pub fn created_at(&self) -> DateTime<Utc> {
        self.events
            .entity_first_persisted_at()
            .expect("entity_first_persisted_at not found")
    }

    pub fn obligation_type(&self) -> ObligationType {
        self.events
            .iter_all()
            .find_map(|e| match e {
                ObligationEvent::Initialized {
                    obligation_type, ..
                } => Some(*obligation_type),
                _ => None,
            })
            .expect("Entity was not Initialized")
    }

    pub fn due_at(&self) -> DateTime<Utc> {
        self.events
            .iter_all()
            .find_map(|e| match e {
                ObligationEvent::Initialized { due_date, .. } => Some(*due_date),
                _ => None,
            })
            .expect("Entity was not Initialized")
    }

    pub fn overdue_at(&self) -> Option<DateTime<Utc>> {
        self.events.iter_all().find_map(|e| match e {
            ObligationEvent::Initialized { overdue_date, .. } => *overdue_date,
            _ => None,
        })
    }

    pub fn not_yet_due_accounts(&self) -> ObligationAccounts {
        self.events
            .iter_all()
            .find_map(|e| match e {
                ObligationEvent::Initialized {
                    not_yet_due_accounts,
                    ..
                } => Some(*not_yet_due_accounts),
                _ => None,
            })
            .expect("Entity was not Initialized")
    }

    pub fn due_accounts(&self) -> ObligationAccounts {
        self.events
            .iter_all()
            .find_map(|e| match e {
                ObligationEvent::Initialized { due_accounts, .. } => Some(*due_accounts),
                _ => None,
            })
            .expect("Entity was not Initialized")
    }

    pub fn overdue_accounts(&self) -> ObligationAccounts {
        self.events
            .iter_all()
            .find_map(|e| match e {
                ObligationEvent::Initialized {
                    overdue_accounts, ..
                } => Some(*overdue_accounts),
                _ => None,
            })
            .expect("Entity was not Initialized")
    }

    pub fn receivable_account_id(&self) -> Option<CalaAccountId> {
        let (not_yet_due_accounts, due_accounts, overdue_accounts) = self
            .events
            .iter_all()
            .find_map(|e| match e {
                ObligationEvent::Initialized {
                    not_yet_due_accounts,
                    due_accounts,
                    overdue_accounts,
                    ..
                } => Some((*not_yet_due_accounts, *due_accounts, *overdue_accounts)),
                _ => None,
            })
            .expect("Entity was not Initialized");

        match self.status() {
            ObligationStatus::NotYetDue => Some(not_yet_due_accounts.receivable_account_id),
            ObligationStatus::Due => Some(due_accounts.receivable_account_id),
            ObligationStatus::Overdue | ObligationStatus::Defaulted => {
                Some(overdue_accounts.receivable_account_id)
            }

            ObligationStatus::Paid => None,
        }
    }

    pub fn account_to_be_credited_id(&self) -> Option<CalaAccountId> {
        let (not_yet_due_accounts, due_accounts, overdue_accounts) = self
            .events
            .iter_all()
            .find_map(|e| match e {
                ObligationEvent::Initialized {
                    not_yet_due_accounts,
                    due_accounts,
                    overdue_accounts,
                    ..
                } => Some((*not_yet_due_accounts, *due_accounts, *overdue_accounts)),
                _ => None,
            })
            .expect("Entity was not Initialized");

        match self.status() {
            ObligationStatus::NotYetDue => Some(not_yet_due_accounts.account_to_be_credited_id),
            ObligationStatus::Due => Some(due_accounts.account_to_be_credited_id),
            ObligationStatus::Overdue | ObligationStatus::Defaulted => {
                Some(overdue_accounts.account_to_be_credited_id)
            }

            ObligationStatus::Paid => None,
        }
    }

    pub fn expected_status(&self) -> ObligationStatus {
        let (due_date, overdue_date, defaulted_date) = self
            .events
            .iter_all()
            .find_map(|e| match e {
                ObligationEvent::Initialized {
                    due_date,
                    overdue_date,
                    defaulted_date,
                    ..
                } => Some((*due_date, *overdue_date, *defaulted_date)),
                _ => None,
            })
            .expect("Entity was not Initialized");

        let now = crate::time::now();

        if let Some(defaulted_date) = defaulted_date {
            if now >= defaulted_date {
                return ObligationStatus::Defaulted;
            }
        }

        if let Some(overdue_date) = overdue_date {
            if now >= overdue_date {
                return ObligationStatus::Overdue;
            }
        }

        if now >= due_date {
            ObligationStatus::Due
        } else {
            ObligationStatus::NotYetDue
        }
    }

    pub fn status(&self) -> ObligationStatus {
        self.events
            .iter_all()
            .rev()
            .find_map(|event| match event {
                ObligationEvent::DueRecorded { .. } => Some(ObligationStatus::Due),
                ObligationEvent::OverdueRecorded { .. } => Some(ObligationStatus::Overdue),
                ObligationEvent::Completed { .. } => Some(ObligationStatus::Paid),
                _ => None,
            })
            .unwrap_or(ObligationStatus::NotYetDue)
    }

    pub fn facility_balance_update_data(&self) -> BalanceUpdateData {
        BalanceUpdateData {
            source_id: self.id.into(),
            ledger_tx_id: self.tx_id,
            balance_type: self.obligation_type(),
            amount: self.initial_amount,
            updated_at: self.recorded_at,
        }
    }

    pub fn outstanding(&self) -> UsdCents {
        self.events
            .iter_all()
            .fold(UsdCents::from(0), |mut total_sum, event| {
                if let ObligationEvent::Initialized { amount, .. } = event {
                    total_sum += *amount;
                }
                total_sum
            })
    }

    pub(crate) fn record_due(
        &mut self,
        audit_info: AuditInfo,
    ) -> Idempotent<ObligationDueReallocationData> {
        idempotency_guard!(
            self.events.iter_all().rev(),
            ObligationEvent::DueRecorded { .. }
        );

        let res = ObligationDueReallocationData {
            tx_id: LedgerTxId::new(),
            amount: self.outstanding(),
            not_yet_due_account_id: self.not_yet_due_accounts().receivable_account_id,
            due_account_id: self.due_accounts().receivable_account_id,
        };

        self.events.push(ObligationEvent::DueRecorded {
            tx_id: res.tx_id,
            audit_info,
        });

        Idempotent::Executed(res)
    }

    pub(crate) fn record_overdue(
        &mut self,
        audit_info: AuditInfo,
    ) -> Result<Idempotent<ObligationOverdueReallocationData>, ObligationError> {
        idempotency_guard!(
            self.events.iter_all().rev(),
            ObligationEvent::OverdueRecorded { .. }
        );

        if self.status() != ObligationStatus::Due {
            return Err(ObligationError::InvalidStatusTransitionToOverdue);
        }

        let res = ObligationOverdueReallocationData {
            tx_id: LedgerTxId::new(),
            outstanding_amount: self.outstanding(),
            due_account_id: self.due_accounts().receivable_account_id,
            overdue_account_id: self.overdue_accounts().receivable_account_id,
        };

        self.events.push(ObligationEvent::OverdueRecorded {
            tx_id: res.tx_id,
            audit_info,
        });

        Ok(Idempotent::Executed(res))
    }
}

impl TryFromEvents<ObligationEvent> for Obligation {
    fn try_from_events(events: EntityEvents<ObligationEvent>) -> Result<Self, EsEntityError> {
        let mut builder = ObligationBuilder::default();
        for event in events.iter_all() {
            match event {
                ObligationEvent::Initialized {
                    id,
                    tx_id,
                    credit_facility_id,
                    reference,
                    amount,
                    recorded_at,
                    ..
                } => {
                    builder = builder
                        .id(*id)
                        .tx_id(*tx_id)
                        .credit_facility_id(*credit_facility_id)
                        .reference(reference.clone())
                        .initial_amount(*amount)
                        .recorded_at(*recorded_at)
                }
                ObligationEvent::DueRecorded { .. } => (),
                ObligationEvent::OverdueRecorded { .. } => (),
                ObligationEvent::Completed { .. } => (),
            }
        }
        builder.events(events).build()
    }
}

#[derive(Debug, Builder)]
pub struct NewObligation {
    #[builder(setter(into))]
    pub(crate) id: ObligationId,
    #[builder(setter(into))]
    pub(crate) tx_id: LedgerTxId,
    #[builder(setter(into))]
    pub(super) credit_facility_id: CreditFacilityId,
    pub(super) obligation_type: ObligationType,
    #[builder(setter(into))]
    pub(super) amount: UsdCents,
    #[builder(setter(strip_option), default)]
    reference: Option<String>,
    not_yet_due_accounts: ObligationAccounts,
    due_accounts: ObligationAccounts,
    due_date: DateTime<Utc>,
    overdue_accounts: ObligationAccounts,
    #[builder(setter(strip_option), default)]
    overdue_date: Option<DateTime<Utc>>,
    #[builder(setter(strip_option), default)]
    defaulted_date: Option<DateTime<Utc>>,
    recorded_at: DateTime<Utc>,
    #[builder(setter(into))]
    pub audit_info: AuditInfo,
}

impl NewObligation {
    pub fn builder() -> NewObligationBuilder {
        NewObligationBuilder::default()
    }

    pub(super) fn reference(&self) -> String {
        match self.reference.as_deref() {
            None => self.id.to_string(),
            Some("") => self.id.to_string(),
            Some(reference) => reference.to_string(),
        }
    }
}

#[derive(Clone)]
pub(super) struct ObligationDataForAllocation {
    pub(super) id: ObligationId,
    pub(super) obligation_type: ObligationType,
    pub(super) recorded_at: DateTime<Utc>,
    pub(super) outstanding: UsdCents,
    pub(super) receivable_account_id: CalaAccountId,
    pub(super) account_to_be_credited_id: CalaAccountId,
}

impl From<&Obligation> for ObligationDataForAllocation {
    fn from(obligation: &Obligation) -> Self {
        Self {
            id: obligation.id,
            obligation_type: obligation.obligation_type(),
            recorded_at: obligation.recorded_at,
            outstanding: obligation.outstanding(),
            receivable_account_id: obligation
                .receivable_account_id()
                .expect("Obligation was already paid"),
            account_to_be_credited_id: obligation
                .account_to_be_credited_id()
                .expect("Obligation was already paid"),
        }
    }
}

impl IntoEvents<ObligationEvent> for NewObligation {
    fn into_events(self) -> EntityEvents<ObligationEvent> {
        EntityEvents::init(
            self.id,
            [ObligationEvent::Initialized {
                id: self.id,
                credit_facility_id: self.credit_facility_id,
                obligation_type: self.obligation_type,
                reference: self.reference(),
                amount: self.amount,
                tx_id: self.tx_id,
                not_yet_due_accounts: self.not_yet_due_accounts,
                due_accounts: self.due_accounts,
                overdue_accounts: self.overdue_accounts,
                due_date: self.due_date,
                overdue_date: self.overdue_date,
                defaulted_date: self.defaulted_date,
                recorded_at: self.recorded_at,
                audit_info: self.audit_info,
            }],
        )
    }
}

#[cfg(test)]
mod test {
    use audit::{AuditEntryId, AuditInfo};

    use super::*;

    fn dummy_audit_info() -> AuditInfo {
        AuditInfo {
            audit_entry_id: AuditEntryId::from(1),
            sub: "sub".to_string(),
        }
    }

    fn obligation_from(events: Vec<ObligationEvent>) -> Obligation {
        Obligation::try_from_events(EntityEvents::init(ObligationId::new(), events)).unwrap()
    }

    fn initial_events() -> Vec<ObligationEvent> {
        vec![ObligationEvent::Initialized {
            id: ObligationId::new(),
            credit_facility_id: CreditFacilityId::new(),
            obligation_type: ObligationType::Disbursal,
            amount: UsdCents::ONE,
            reference: "ref-01".to_string(),
            tx_id: LedgerTxId::new(),
            not_yet_due_accounts: ObligationAccounts {
                receivable_account_id: CalaAccountId::new(),
                account_to_be_credited_id: CalaAccountId::new(),
            },
            due_accounts: ObligationAccounts {
                receivable_account_id: CalaAccountId::new(),
                account_to_be_credited_id: CalaAccountId::new(),
            },
            overdue_accounts: ObligationAccounts {
                receivable_account_id: CalaAccountId::new(),
                account_to_be_credited_id: CalaAccountId::new(),
            },
            due_date: Utc::now(),
            overdue_date: Some(Utc::now()),
            defaulted_date: None,
            recorded_at: Utc::now(),
            audit_info: dummy_audit_info(),
        }]
    }

    #[test]
    fn record_overdue() {
        let mut obligation = obligation_from(initial_events());
        let _ = obligation.record_due(dummy_audit_info());
        let res = obligation
            .record_overdue(dummy_audit_info())
            .unwrap()
            .unwrap();
        assert_eq!(res.outstanding_amount, obligation.initial_amount);
    }

    #[test]
    fn errors_if_overdue_recorded_before_due() {
        let mut obligation = obligation_from(initial_events());
        let res = obligation.record_overdue(dummy_audit_info());
        assert!(matches!(
            res,
            Err(ObligationError::InvalidStatusTransitionToOverdue)
        ));
    }
}
