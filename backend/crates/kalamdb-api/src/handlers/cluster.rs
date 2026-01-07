use actix_web::{post, web, HttpRequest, HttpResponse, Responder};
use kalamdb_commons::cluster::LiveQueryBroadcast;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::live::{ChangeNotification, ChangeType};
use kalamdb_core::providers::arrow_json_conversion::{coerce_rows, json_to_row};
use std::sync::Arc;

#[post("/cluster/live_query_notify")]
pub async fn live_query_notify(
    http_req: HttpRequest,
    payload: web::Json<LiveQueryBroadcast>,
    app_context: web::Data<Arc<AppContext>>,
) -> impl Responder {
    let cluster_config = match &app_context.config().cluster {
        Some(config) => config,
        None => {
            return HttpResponse::NotFound().finish();
        }
    };

    let header_cluster_id = http_req
        .headers()
        .get("X-Cluster-Id")
        .and_then(|value| value.to_str().ok());

    if header_cluster_id != Some(cluster_config.cluster_id.as_str())
        || payload.cluster_id != cluster_config.cluster_id
    {
        return HttpResponse::Unauthorized().finish();
    }

    let change_type = match payload.change_type.to_ascii_lowercase().as_str() {
        "insert" => ChangeType::Insert,
        "update" => ChangeType::Update,
        "delete" => ChangeType::Delete,
        "flush" => ChangeType::Flush,
        _ => return HttpResponse::BadRequest().finish(),
    };

    let row = match json_to_row(&payload.row_data) {
        Some(row) => row,
        None => return HttpResponse::BadRequest().finish(),
    };
    let old_row = if let Some(value) = payload.old_data.as_ref() {
        match json_to_row(value) {
            Some(row) => Some(row),
            None => return HttpResponse::BadRequest().finish(),
        }
    } else {
        None
    };

    let (row_data, old_data) = if change_type == ChangeType::Flush {
        (row, old_row)
    } else {
        let schema = match app_context.schema_registry().get_arrow_schema(&payload.table_id) {
            Ok(schema) => schema,
            Err(_) => return HttpResponse::BadRequest().finish(),
        };

        let mut rows = match coerce_rows(vec![row], &schema) {
            Ok(rows) => rows,
            Err(_) => return HttpResponse::BadRequest().finish(),
        };
        let row_data = match rows.pop() {
            Some(row) => row,
            None => return HttpResponse::BadRequest().finish(),
        };

        let old_data = if let Some(old_row) = old_row {
            let mut old_rows = match coerce_rows(vec![old_row], &schema) {
                Ok(rows) => rows,
                Err(_) => return HttpResponse::BadRequest().finish(),
            };
            match old_rows.pop() {
                Some(row) => Some(row),
                None => return HttpResponse::BadRequest().finish(),
            }
        } else {
            None
        };

        (row_data, old_data)
    };

    let notification = ChangeNotification {
        change_type,
        table_id: payload.table_id.clone(),
        row_data,
        old_data,
        row_id: payload.row_id.clone(),
    };

    app_context
        .live_query_manager()
        .notify_table_change_local_async(
            payload.user_id.clone(),
            payload.table_id.clone(),
            notification,
        );

    HttpResponse::Ok().finish()
}
