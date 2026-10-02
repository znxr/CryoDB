use super::{Context, Message, Output, State};
use crate::model::connection::ConnectionInfo;
use crate::model::transfer::{ImportSummary, TransferError, TransferStage};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

fn update(state: &mut State, message: Message) -> Vec<Output> {
    let connection = ConnectionInfo {
        driver: crate::model::connection::DatabaseDriver::MySql,
        host: String::new(),
        port: String::new(),
        database: String::new(),
        username: String::new(),
        password: String::new(),
        sqlite_path: String::new(),
        tls_mode: crate::model::connection::TlsMode::Prefer,
        tls_ca_cert_path: String::new(),
        tls_client_cert_path: String::new(),
        tls_client_key_path: String::new(),
    };
    state
        .update(
            message,
            Context {
                pool: None,
                connection: &connection,
                database: None,
                is_connected: false,
            },
        )
        .1
}

#[test]
fn selecting_filtered_tables_preserves_the_rest() {
    let mut state = State {
        export_tables: vec!["users".into(), "orders".into()],
        export_table_search: "user".into(),
        ..State::default()
    };
    update(&mut state, Message::ExportSelectNoneTables);
    assert_eq!(state.export_selected_tables(), vec!["orders"]);
    update(&mut state, Message::ExportSelectAllTables);
    assert_eq!(state.export_selected_tables(), vec!["users", "orders"]);
}

#[test]
fn cancellation_signals_the_worker_without_resetting_running_state() {
    let flag = Arc::new(AtomicBool::new(false));
    let mut state = State {
        import_stage: TransferStage::Running,
        import_cancel_flag: Some(flag.clone()),
        ..State::default()
    };
    update(&mut state, Message::CancelImport);
    assert!(flag.load(Ordering::Relaxed));
    assert_eq!(state.import_stage, TransferStage::Running);
    update(&mut state, Message::CloseImportModal);
    assert_eq!(state.import_stage, TransferStage::Running);
    update(
        &mut state,
        Message::ImportFinished(Err(TransferError::Cancelled)),
    );
    assert_eq!(state.import_stage, TransferStage::Completed);
    assert!(state.import_cancel_flag.is_none());
    assert_eq!(state.import_status, "Import cancelled.");
}

#[test]
fn completion_prevents_later_progress_from_overwriting_the_result() {
    let mut state = State {
        import_stage: TransferStage::Running,
        ..State::default()
    };
    update(
        &mut state,
        Message::ImportProgress {
            progress: 2.0,
            status: "Importing".into(),
        },
    );
    assert_eq!(state.import_progress, 1.0);
    update(
        &mut state,
        Message::ImportFinished(Ok(ImportSummary {
            statements: 3,
            duration: Duration::from_secs(1),
        })),
    );
    let completed_status = state.import_status.clone();
    update(
        &mut state,
        Message::ImportProgress {
            progress: 0.2,
            status: "Late progress".into(),
        },
    );
    assert_eq!(state.import_stage, TransferStage::Completed);
    assert_eq!(state.import_status, completed_status);
    assert_eq!(state.import_progress, 1.0);
}

#[test]
fn starting_without_a_connection_reports_an_error_without_running() {
    let mut state = State::default();
    let outputs = update(&mut state, Message::StartImport);
    assert!(
        outputs
            .iter()
            .any(|output| matches!(output, Output::Error(Some(_))))
    );
    assert_eq!(state.import_stage, TransferStage::Configure);
    assert!(state.import_cancel_flag.is_none());
}
