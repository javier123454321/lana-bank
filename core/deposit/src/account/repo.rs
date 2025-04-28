use sqlx::PgPool;

use es_entity::*;
use outbox::OutboxEventMarker;

use crate::{
    event::CoreDepositEvent,
    primitives::{DepositAccountHolderId, DepositAccountId},
    publisher::DepositPublisher,
};

use super::{entity::*, error::*};

#[derive(EsRepo)]
#[es_repo(
    entity = "DepositAccount",
    err = "DepositAccountError",
    columns(account_holder_id(ty = "DepositAccountHolderId", list_for, update(persist = false))),
    tbl_prefix = "core",
    post_persist_hook = "publish"
)]
pub struct DepositAccountRepo<E>
where
    E: OutboxEventMarker<CoreDepositEvent>,
{
    publisher: DepositPublisher<E>,
    #[allow(dead_code)]
    pool: PgPool,
}

impl<E> Clone for DepositAccountRepo<E>
where
    E: OutboxEventMarker<CoreDepositEvent>,
{
    fn clone(&self) -> Self {
        Self {
            publisher: self.publisher.clone(),
            pool: self.pool.clone(),
        }
    }
}

impl<E> DepositAccountRepo<E>
where
    E: OutboxEventMarker<CoreDepositEvent>,
{
    pub fn new(pool: &PgPool, publisher: &DepositPublisher<E>) -> Self {
        Self {
            pool: pool.clone(),
            publisher: publisher.clone(),
        }
    }

    async fn publish(
        &self,
        db: &mut es_entity::DbOp<'_>,
        entity: &DepositAccount,
        new_events: es_entity::LastPersisted<'_, DepositAccountEvent>,
    ) -> Result<(), DepositAccountError> {
        self.publisher
            .publish_deposit_account(db, entity, new_events)
            .await
    }
}
