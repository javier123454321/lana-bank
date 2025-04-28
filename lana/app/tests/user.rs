mod helpers;
use rand::distr::{Alphanumeric, SampleString};
use serial_test::file_serial;

use lana_app::{audit::*, authorization::init as init_authz, primitives::*};

fn generate_random_email() -> String {
    let random_string: String = Alphanumeric.sample_string(&mut rand::rng(), 32);
    format!("{}@example.com", random_string.to_lowercase())
}

#[tokio::test]
#[file_serial]
async fn bank_manager_lifecycle() -> anyhow::Result<()> {
    let pool = helpers::init_pool().await?;
    let audit = Audit::new(&pool);
    let authz = init_authz(&pool, &audit).await?;
    let (users, superuser_subject) = helpers::init_users(&pool, &authz).await?;

    let user_email = generate_random_email();
    let user = users
        .create_user(&superuser_subject, user_email.clone())
        .await?;
    assert_eq!(user.email, user_email);
    assert_eq!(user.current_roles().len(), 0);

    let bank_manager = users
        .assign_role_to_user(&superuser_subject, user.id, LanaRole::BANK_MANAGER)
        .await
        .expect("Could not assign role to user");

    assert_eq!(bank_manager.id, user.id);
    let roles: Vec<_> = bank_manager.current_roles().into_iter().collect();
    assert_eq!(roles, vec![LanaRole::BANK_MANAGER]);

    let user = users
        .revoke_role_from_user(&superuser_subject, bank_manager.id, LanaRole::BANK_MANAGER)
        .await?;

    assert_eq!(user.current_roles().len(), 0);

    Ok(())
}
