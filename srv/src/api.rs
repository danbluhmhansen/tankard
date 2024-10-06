use std::{collections::HashMap, str::FromStr};

use axum::{
    async_trait,
    body::Body,
    extract::{FromRequest, FromRequestParts, Path, Request, State},
    http::{header::CONTENT_TYPE, request::Parts, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Extension, Form, Json, RequestExt, Router,
};
use axum_extra::{extract::JsonLines, TypedHeader};
use futures::{StreamExt, TryStreamExt};
use itertools::Itertools;
use mediatype::{media_type, MediaType};
use serde::{Deserialize, Serialize};
use tokio_postgres::types::Type;

use crate::{internal_error, AppState};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Column {
    column_name: String,
    data_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Table {
    name: String,
    columns: Vec<Column>,
}

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for Table {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(name) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| StatusCode::NOT_FOUND.into_response())?;
        let Extension(tables) =
            Extension::<HashMap<String, Vec<Column>>>::from_request_parts(parts, state)
                .await
                // TODO: add error description
                .map_err(|e| internal_error(e).into_response())?;
        tables
            .get(&name)
            // TODO: avoid clone?
            .cloned()
            .map(|columns| Self { name, columns })
            .ok_or(StatusCode::NOT_FOUND.into_response())
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Select(pub(crate) Vec<String>);

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for Select {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let select = match parts.uri.query().map(Select::from_str) {
            Some(Ok(select)) => Ok(select),
            Some(Err(err)) => Err((StatusCode::INTERNAL_SERVER_ERROR, err).into_response()),
            None => Ok(Self(vec![])),
        }?;
        let table = Table::from_request_parts(parts, state).await?;

        let bad_selects: Vec<_> = select
            .0
            .iter()
            .filter(|&s| !table.columns.iter().any(|c| &c.column_name == s))
            .map(|s| s.as_str())
            .collect();

        if bad_selects.is_empty() {
            Ok(select)
        } else {
            // TODO: better error response
            Err((StatusCode::BAD_REQUEST, bad_selects.join(",")).into_response())
        }
    }
}

pub(crate) struct JsonOrForm<T>(T);

#[async_trait]
impl<S: Send + Sync, T: 'static> FromRequest<S> for JsonOrForm<T>
where
    Json<T>: FromRequest<()>,
    Form<T>: FromRequest<()>,
{
    type Rejection = Response;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let content_type_header = req.headers().get(CONTENT_TYPE);
        let content_type = content_type_header.and_then(|value| value.to_str().ok());

        match content_type {
            Some(content_type) if content_type.starts_with("application/json") => req
                .extract()
                .await
                .map(|Json(payload)| Self(payload))
                .map_err(IntoResponse::into_response),
            Some(content_type) if content_type.starts_with("application/x-www-form-urlencoded") => {
                req.extract()
                    .await
                    .map(|Form(payload)| Self(payload))
                    .map_err(IntoResponse::into_response)
            }
            _ => Err(StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response()),
        }
    }
}

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/", get(get_table).post(set_table))
}

const MT_TEXT_HTML: MediaType = media_type!(TEXT / HTML);
const MT_APPLICATION_JSON: MediaType = media_type!(APPLICATION / JSON);
const MT_TEXT_CSV: MediaType = media_type!(TEXT / CSV);

const AVAILABLE: &[MediaType] = &[MT_TEXT_HTML, MT_APPLICATION_JSON, MT_TEXT_CSV];

pub(crate) async fn get_table(
    Table {
        name: table,
        columns,
    }: Table,
    Select(select): Select,
    TypedHeader(accept): TypedHeader<headers_accept::Accept>,
    State(AppState { pool, .. }): State<AppState>,
) -> Response {
    let conn = match pool.get().await.map_err(internal_error) {
        Ok(conn) => conn,
        Err(err) => return err.into_response(),
    };
    match accept.negotiate(AVAILABLE) {
        Some(mt) if mt == &MT_APPLICATION_JSON => {
            let select = if !select.is_empty() {
                &format!(
                    "jsonb_build_object({})",
                    select.iter().map(|s| format!("'{s}', {s}")).join(",")
                )
            } else {
                &format!("to_json({table})")
            };
            conn.copy_out(&(format!("copy (select {select} from {table}) to stdout;")))
                .await
                .map(|stream| {
                    JsonLines::new(
                        stream
                            .map_ok(|b| serde_json::from_slice::<serde_json::Value>(&b))
                            .filter_map(|res| async { res.ok() }),
                    )
                })
                .map_err(internal_error)
                .into_response()
        }
        Some(mt) if mt == &MT_TEXT_CSV => {
            let select = if !select.is_empty() {
                &select.join(",")
            } else {
                "*"
            };

            conn.copy_out(
                &(format!("copy (select {select} from {table}) to stdout with csv header;")),
            )
            .await
            .map(Body::from_stream)
            .map_err(internal_error)
            .into_response()
        }
        Some(mt) if mt == &MT_TEXT_HTML => {
            let head = if !select.is_empty() {
                select.iter().map(|s| format!("'{s}'")).join(",")
            } else {
                columns
                    .iter()
                    .map(|Column { column_name, .. }| format!("'{column_name}'"))
                    .join(",")
            };

            let select = if !select.is_empty() {
                &select.iter().map(|s| format!("{s}::text")).join(",")
            } else {
                &table
            };

            // TODO: support other keys than `id`
            conn.query_one(&(format!(
                "select html_minify(jinja_render($1, (select jsonb_build_object('head', array[{head}], 'body', (select array_agg(jsonb_build_object('key', id, 'cols', array[{select}])) from {table})))));"
            )), &[&include_str!("../../tmpl/table.html")])
            .await
            .map(|row| Html(row.get::<_, String>(0)))
            .map_err(internal_error)
            .into_response()
        }
        _ => StatusCode::NOT_ACCEPTABLE.into_response(),
    }
}

pub(crate) async fn set_table(
    Table {
        name: table,
        columns,
    }: Table,
    Select(select): Select,
    TypedHeader(accept): TypedHeader<headers_accept::Accept>,
    State(AppState { pool, .. }): State<AppState>,
    JsonOrForm(payload): JsonOrForm<serde_json::Value>,
) -> Response {
    let conn = match pool.get().await.map_err(internal_error) {
        Ok(conn) => conn,
        Err(err) => return err.into_response(),
    };
    // TODO: do not map db genarated columns
    // TODO: coalesce primary key inserts
    let json_cols = columns
        .iter()
        .map(
            |Column {
                 column_name,
                 data_type,
             }| format!("{column_name} {data_type}"),
        )
        .join(",");
    let ins_cols = columns
        .iter()
        .filter(|Column { column_name, .. }| {
            column_name != "id" && column_name != "added" && column_name != "updated"
        })
        .map(|Column { column_name, .. }| column_name)
        .join(",");
    let ins_col_vals = columns
        .iter()
        .filter(|Column { column_name, .. }| {
            column_name != "id" && column_name != "added" && column_name != "updated"
        })
        .map(|Column { column_name, .. }| format!("i.{column_name}"))
        .join(",");
    let upd_cols = columns
        .iter()
        .filter(|Column { column_name, .. }| {
            column_name != "id" && column_name != "added" && column_name != "updated"
        })
        .map(|Column { column_name, .. }| format!("{column_name} = i.{column_name}"))
        .join(",");
    let select = if !select.is_empty() {
        &format!(
            "jsonb_build_object({})",
            select.iter().map(|s| format!("'{s}', e.{s}")).join(",")
        )
    } else {
        "to_json(e.*)"
    };
    match accept.negotiate(AVAILABLE) {
        Some(mt) if mt == &MT_APPLICATION_JSON => {
            // TODO: validate input
            let statement = format!(
                r#"
                    merge into {table} e
                    using (select * from json_table($1, '$' columns (_drop bool,{json_cols}))) i
                    on e.id = i.id
                    when not matched then insert ({ins_cols}) values ({ins_col_vals})
                    when matched and i._drop = false then update set {upd_cols}
                    when matched then delete
                    returning {select};
                "#,
            );
            conn.query_typed_raw(&statement, [(payload, Type::JSONB)])
                .await
                .map(|stream| {
                    JsonLines::new(
                        stream.map(|res| res.map(|row| row.get::<_, serde_json::Value>(0))),
                    )
                })
                .map_err(internal_error)
                .into_response()
        }
        _ => StatusCode::NOT_ACCEPTABLE.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use axum::{
        body::Body,
        extract::Request,
        http::{
            header::{ACCEPT, CONTENT_TYPE},
            StatusCode,
        },
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::tests::setup_app;

    #[tokio::test]
    async fn users_html() -> Result<(), Box<dyn Error>> {
        let (conn, app) = setup_app("1dc4392f-7e60-4a64-8a3d-1788b2ac9820").await?;
        conn.batch_execute(
            "insert into users (id, username, salt, passhash, email) values ('00000000-0000-0000-0000-000000000000', 'one', '', '', 'foo'), ('00000000-0000-0000-0000-000000000001', 'two', '', '', 'foo'), ('00000000-0000-0000-0000-000000000002', 'three', '', '', 'foo');",
        ).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username,email")
                    .header(ACCEPT, "text/html")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
        "<table><thead><tr><th scope=col>username<th scope=col>email<tbody><tr id=00000000-0000-0000-0000-000000000000><td>one<td>foo<tr id=00000000-0000-0000-0000-000000000001><td>two<td>foo<tr id=00000000-0000-0000-0000-000000000002><td>three<td>foo</table>",
        response.into_body().collect().await?.to_bytes()
    );

        Ok(())
    }

    #[tokio::test]
    async fn users_html_empty() -> Result<(), Box<dyn Error>> {
        let (_, app) = setup_app("b3a21a9d-2b18-466e-8e4d-c57d2f52c232").await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username,email")
                    .header(ACCEPT, "text/html")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!("", response.into_body().collect().await?.to_bytes());

        Ok(())
    }

    #[tokio::test]
    async fn users_json() -> Result<(), Box<dyn Error>> {
        let (conn, app) = setup_app("573f05dc-baeb-4069-9f38-93f6279be1a8").await?;
        conn.batch_execute(
            "insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');",
        ).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username")
                    .header(ACCEPT, "application/json")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            "{\"username\":\"one\"}\n{\"username\":\"two\"}\n{\"username\":\"three\"}\n",
            response.into_body().collect().await?.to_bytes()
        );

        Ok(())
    }

    #[tokio::test]
    async fn users_json_empty() -> Result<(), Box<dyn Error>> {
        let (_, app) = setup_app("de4266a6-125a-4998-8f3a-f661c8c925ad").await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username")
                    .header(ACCEPT, "application/json")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!("", response.into_body().collect().await?.to_bytes());

        Ok(())
    }

    #[tokio::test]
    async fn users_csv() -> Result<(), Box<dyn Error>> {
        let (conn, app) = setup_app("f52d7679-f08d-4830-9cbb-11cf4dce6742").await?;
        conn.batch_execute(
            "insert into users (username, salt, passhash, email) values ('one', '', '', 'foo'), ('two', '', '', 'foo'), ('three', '', '', 'foo');",
        ).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username,email")
                    .header(ACCEPT, "text/csv")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await?.to_bytes(),
            "username,email\none,foo\ntwo,foo\nthree,foo\n"
        );

        Ok(())
    }

    #[tokio::test]
    async fn users_csv_empty() -> Result<(), Box<dyn Error>> {
        let (_, app) = setup_app("d822e1bb-d6b4-46f6-8a37-77ea3bab1ecb").await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=username,email")
                    .header(ACCEPT, "text/csv")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await?.to_bytes(),
            "username,email\n"
        );

        Ok(())
    }

    #[tokio::test]
    async fn users_bad_select() -> Result<(), Box<dyn Error>> {
        let (conn, app) = setup_app("497f06e4-c65d-4e67-8a3c-006f772819f7").await?;
        conn.batch_execute(
            "insert into users (username, salt, passhash) values ('one', '', ''), ('two', '', ''), ('three', '', '');",
        ).await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/users?select=bad_column")
                    .header(ACCEPT, "text/html")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.into_body().collect().await?.to_bytes(),
            "bad_column"
        );

        Ok(())
    }

    #[tokio::test]
    async fn table_not_found() -> Result<(), Box<dyn Error>> {
        let (_, app) = setup_app("c780c975-31b3-4bfa-9945-fc136757779a").await?;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/no_table")
                    .header(ACCEPT, "text/html")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    #[tokio::test]
    async fn users_post_json() -> Result<(), Box<dyn Error>> {
        let (_, app) = setup_app("d54eddd9-6a92-46fd-9c58-0c5a9b710312").await?;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/users?select=username")
                    .header(ACCEPT, "application/json")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"[{"username":"one","salt":"","passhash":""}]"#,
                    ))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            "{\"username\":\"one\"}\n",
            response.into_body().collect().await?.to_bytes()
        );

        Ok(())
    }
}
