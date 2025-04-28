mod helpers;

use rand::Rng;

use authz::dummy::DummySubject;
use cala_ledger::{CalaLedger, CalaLedgerConfig};
use cloud_storage::{config::StorageConfig, Storage};

use core_accounting::CoreAccounting;
use core_credit::*;
use helpers::{action, event, object};

#[tokio::test]
async fn chart_of_accounts_integration() -> anyhow::Result<()> {
    let pool = helpers::init_pool().await?;

    let outbox = outbox::Outbox::<event::DummyEvent>::init(&pool).await?;
    let authz = authz::dummy::DummyPerms::<action::DummyAction, object::DummyObject>::new();

    let governance = governance::Governance::new(&pool, &authz, &outbox);
    let customers = core_customer::Customers::new(&pool, &authz, &outbox);
    let price = core_price::Price::new();

    let cala_config = CalaLedgerConfig::builder()
        .pool(pool.clone())
        .exec_migrations(false)
        .build()?;
    let cala = CalaLedger::init(cala_config).await?;
    let jobs = job::Jobs::new(&pool, job::JobExecutorConfig::default());

    let journal_id = helpers::init_journal(&cala).await?;

    let credit = CoreCredit::init(
        &pool,
        Default::default(),
        &governance,
        &jobs,
        &authz,
        &customers,
        &price,
        &outbox,
        &cala,
        journal_id,
    )
    .await?;

    let storage = Storage::new(&StorageConfig::default());
    let accounting = CoreAccounting::new(&pool, &authz, &cala, journal_id, &storage, &jobs);
    let chart_ref = format!("ref-{:08}", rand::rng().random_range(0..10000));
    let chart = accounting
        .chart_of_accounts()
        .create_chart(&DummySubject, "Test chart".to_string(), chart_ref)
        .await?;
    let import = r#"
        1,Facility Omnibus Parent
        2,Collateral Omnibus Parent
        3,Facility Parent
        4,Collateral Parent
        5,Disbursed Receivable Parent
        6,Interest Receivable Parent
        7,Interest Income Parent
        8,Fee Income Parent
        "#
    .to_string();
    let chart_id = chart.id;
    let chart = accounting
        .chart_of_accounts()
        .import_from_csv(&DummySubject, chart_id, import)
        .await?;

    let code = "1".parse::<core_accounting::AccountCode>().unwrap();
    let account_set_id = cala
        .account_sets()
        .find(chart.account_set_id_from_code(&code).unwrap())
        .await?
        .id;

    credit
        .set_chart_of_accounts_integration_config(
            &DummySubject,
            &chart,
            ChartOfAccountsIntegrationConfig::builder()
                .chart_of_accounts_id(chart_id)
                .chart_of_account_facility_omnibus_parent_code("1".parse().unwrap())
                .chart_of_account_collateral_omnibus_parent_code("2".parse().unwrap())
                .chart_of_account_facility_parent_code("3".parse().unwrap())
                .chart_of_account_collateral_parent_code("4".parse().unwrap())
                .chart_of_account_interest_income_parent_code("7".parse().unwrap())
                .chart_of_account_fee_income_parent_code("8".parse().unwrap())
                .chart_of_account_short_term_individual_disbursed_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_short_term_government_entity_disbursed_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_short_term_private_company_disbursed_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_short_term_bank_disbursed_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_short_term_financial_institution_disbursed_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_short_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_short_term_non_domiciled_company_disbursed_receivable_parent_code(
                    "7".parse().unwrap(),
                )
                .chart_of_account_long_term_individual_disbursed_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_long_term_government_entity_disbursed_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_long_term_private_company_disbursed_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_long_term_bank_disbursed_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_long_term_financial_institution_disbursed_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_long_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_long_term_non_domiciled_company_disbursed_receivable_parent_code(
                    "7".parse().unwrap(),
                )

                .chart_of_account_short_term_individual_interest_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_short_term_government_entity_interest_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_short_term_private_company_interest_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_short_term_bank_interest_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_short_term_financial_institution_interest_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_short_term_foreign_agency_or_subsidiary_interest_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_short_term_non_domiciled_company_interest_receivable_parent_code(
                    "7".parse().unwrap(),
                )
                .chart_of_account_long_term_individual_interest_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_long_term_government_entity_interest_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_long_term_private_company_interest_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_long_term_bank_interest_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_long_term_financial_institution_interest_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_long_term_foreign_agency_or_subsidiary_interest_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_long_term_non_domiciled_company_interest_receivable_parent_code(
                    "7".parse().unwrap(),
                )
                .chart_of_account_overdue_individual_disbursed_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_overdue_government_entity_disbursed_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_overdue_private_company_disbursed_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_overdue_bank_disbursed_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_overdue_financial_institution_disbursed_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_overdue_foreign_agency_or_subsidiary_disbursed_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_overdue_non_domiciled_company_disbursed_receivable_parent_code(
                    "7".parse().unwrap(),
                )
                .build()
                .unwrap(),
        )
        .await?;

    let res = cala
        .account_sets()
        .list_members_by_created_at(account_set_id, Default::default())
        .await?;

    assert_eq!(res.entities.len(), 6);

    let chart_ref = format!("other-ref-{:08}", rand::rng().random_range(0..10000));
    let chart = accounting
        .chart_of_accounts()
        .create_chart(&DummySubject, "Other Test chart".to_string(), chart_ref)
        .await?;

    let import = r#"
        1,Other Facility Omnibus Parent
        2,Other Collateral Omnibus Parent
        3,Other Facility Parent
        4,Other Collateral Parent
        5,Other Disbursed Receivable Parent
        6,Other Interest Receivable Parent
        7,Other Interest Income Parent
        8,Other Fee Income Parent
        "#
    .to_string();
    let chart_id = chart.id;
    let chart = accounting
        .chart_of_accounts()
        .import_from_csv(&DummySubject, chart_id, import)
        .await?;

    let res = credit
        .set_chart_of_accounts_integration_config(
            &DummySubject,
            &chart,
            ChartOfAccountsIntegrationConfig::builder()
                .chart_of_accounts_id(chart_id)
                .chart_of_account_facility_omnibus_parent_code("1".parse().unwrap())
                .chart_of_account_collateral_omnibus_parent_code("2".parse().unwrap())
                .chart_of_account_facility_parent_code("3".parse().unwrap())
                .chart_of_account_collateral_parent_code("4".parse().unwrap())
                .chart_of_account_interest_income_parent_code("7".parse().unwrap())
                .chart_of_account_fee_income_parent_code("8".parse().unwrap())
                .chart_of_account_short_term_individual_disbursed_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_short_term_government_entity_disbursed_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_short_term_private_company_disbursed_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_short_term_bank_disbursed_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_short_term_financial_institution_disbursed_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_short_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_short_term_non_domiciled_company_disbursed_receivable_parent_code(
                    "7".parse().unwrap(),
                )
                .chart_of_account_long_term_individual_disbursed_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_long_term_government_entity_disbursed_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_long_term_private_company_disbursed_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_long_term_bank_disbursed_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_long_term_financial_institution_disbursed_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_long_term_foreign_agency_or_subsidiary_disbursed_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_long_term_non_domiciled_company_disbursed_receivable_parent_code(
                    "7".parse().unwrap(),
                )
                                .chart_of_account_short_term_individual_interest_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_short_term_government_entity_interest_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_short_term_private_company_interest_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_short_term_bank_interest_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_short_term_financial_institution_interest_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_short_term_foreign_agency_or_subsidiary_interest_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_short_term_non_domiciled_company_interest_receivable_parent_code(
                    "7".parse().unwrap(),
                )
                .chart_of_account_long_term_individual_interest_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_long_term_government_entity_interest_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_long_term_private_company_interest_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_long_term_bank_interest_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_long_term_financial_institution_interest_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_long_term_foreign_agency_or_subsidiary_interest_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_long_term_non_domiciled_company_interest_receivable_parent_code(
                    "7".parse().unwrap(),
                )
                .chart_of_account_overdue_individual_disbursed_receivable_parent_code("1".parse().unwrap())
                .chart_of_account_overdue_government_entity_disbursed_receivable_parent_code(
                    "2".parse().unwrap(),
                )
                .chart_of_account_overdue_private_company_disbursed_receivable_parent_code(
                    "3".parse().unwrap(),
                )
                .chart_of_account_overdue_bank_disbursed_receivable_parent_code("4".parse().unwrap())
                .chart_of_account_overdue_financial_institution_disbursed_receivable_parent_code(
                    "5".parse().unwrap(),
                )
                .chart_of_account_overdue_foreign_agency_or_subsidiary_disbursed_receivable_parent_code(
                    "6".parse().unwrap(),
                )
                .chart_of_account_overdue_non_domiciled_company_disbursed_receivable_parent_code(
                    "7".parse().unwrap(),
                )

                .build()
                .unwrap(),
        )
        .await;

    assert!(matches!(
        res,
        Err(core_credit::error::CoreCreditError::CreditConfigAlreadyExists)
    ));

    Ok(())
}
