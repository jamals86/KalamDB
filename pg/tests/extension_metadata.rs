use pg_kalam::{pg_kalam_compiled_mode, pg_kalam_user_id_guc_name, pg_kalam_version};

#[test]
fn extension_reports_version_and_guc_name() {
    assert_eq!(pg_kalam_version(), env!("CARGO_PKG_VERSION"));
    assert_eq!(pg_kalam_user_id_guc_name(), "kalam.user_id");
    assert_eq!(pg_kalam_compiled_mode(), "remote");
}
