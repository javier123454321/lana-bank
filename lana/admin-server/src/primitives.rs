#![allow(clippy::upper_case_acronyms)]

use async_graphql::*;
use serde::{Deserialize, Serialize};

pub use lana_app::{
    primitives::{
        ApprovalProcessId, ChartId, CommitteeId, CreditFacilityId, CustomerId, DepositAccountId,
        DepositId, DisbursalId, DisbursalStatus, DocumentId, LanaRole, LedgerTransactionId,
        ManualTransactionId, PaymentId, PolicyId, ReportId, ReportProgress, Satoshis,
        SignedSatoshis, SignedUsdCents, Subject, TermsTemplateId, UsdCents, UserId, WithdrawalId,
    },
    terms::CollateralizationState,
};

pub use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AdminAuthContext {
    pub sub: Subject,
}

impl AdminAuthContext {
    pub fn new(sub: impl Into<UserId>) -> Self {
        Self {
            sub: Subject::User(sub.into()),
        }
    }
}

pub use es_entity::graphql::UUID;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);
scalar!(Timestamp);
impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(value: chrono::DateTime<chrono::Utc>) -> Self {
        Self(value)
    }
}
impl Timestamp {
    #[allow(dead_code)]
    pub fn into_inner(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Date(chrono::NaiveDate);
scalar!(Date);
impl From<chrono::NaiveDate> for Date {
    fn from(value: chrono::NaiveDate) -> Self {
        Self(value)
    }
}
impl Date {
    #[allow(dead_code)]
    pub fn into_inner(self) -> chrono::NaiveDate {
        self.0
    }
}

#[derive(SimpleObject)]
pub struct SuccessPayload {
    pub success: bool,
}

impl From<()> for SuccessPayload {
    fn from(_: ()) -> Self {
        SuccessPayload { success: true }
    }
}

pub trait ToGlobalId {
    fn to_global_id(&self) -> async_graphql::types::ID;
}

macro_rules! impl_to_global_id {
    ($($ty:ty),*) => {
        $(
            impl ToGlobalId for $ty {
                fn to_global_id(&self) -> async_graphql::types::ID {
                    async_graphql::types::ID::from(format!("{}:{}", stringify!($ty).trim_end_matches("Id"), self))
                }
            }
        )*
    };
}

impl_to_global_id! {
    UserId,
    CustomerId,
    ChartId,
    TermsTemplateId,
    CreditFacilityId,
    DisbursalId,
    PaymentId,
    audit::AuditEntryId,
    ReportId,
    DocumentId,
    PolicyId,
    CommitteeId,
    WithdrawalId,
    DepositId,
    ManualTransactionId,
    ApprovalProcessId,
    DepositAccountId,
    LedgerTransactionId
}

use cala_ledger::EntryId;
impl_to_global_id!(EntryId);
