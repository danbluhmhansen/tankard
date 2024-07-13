use std::error::Error;

use pgrx::{pg_sys::panic::ErrorReportable, prelude::*};

#[pg_extern]
fn json_diff(left: pgrx::Json, right: pgrx::Json) -> serde_json::Result<pgrx::Json> {
    Ok(pgrx::Json(serde_json::to_value(json_patch::diff(
        &left.0, &right.0,
    ))?))
}

#[pg_extern]
fn jsonb_diff(left: pgrx::JsonB, right: pgrx::JsonB) -> serde_json::Result<pgrx::JsonB> {
    Ok(pgrx::JsonB(serde_json::to_value(json_patch::diff(
        &left.0, &right.0,
    ))?))
}

#[pg_extern]
fn json_patch(mut doc: pgrx::Json, patch: pgrx::Json) -> Result<pgrx::Json, Box<dyn Error>> {
    let patch = serde_json::from_value::<json_patch::Patch>(patch.0)?;
    json_patch::patch(&mut doc.0, &patch)?;
    Ok(doc)
}

#[pg_extern]
fn jsonb_patch(mut doc: pgrx::JsonB, patch: pgrx::JsonB) -> Result<pgrx::JsonB, Box<dyn Error>> {
    let patch = serde_json::from_value::<json_patch::Patch>(patch.0)?;
    json_patch::patch(&mut doc.0, &patch)?;
    Ok(doc)
}

#[pg_extern]
fn json_merge(mut doc: pgrx::Json, patch: pgrx::Json) -> Result<pgrx::Json, Box<dyn Error>> {
    json_patch::merge(&mut doc.0, &patch.0);
    Ok(doc)
}

#[pg_extern]
fn jsonb_merge(mut doc: pgrx::JsonB, patch: pgrx::JsonB) -> Result<pgrx::JsonB, Box<dyn Error>> {
    json_patch::merge(&mut doc.0, &patch.0);
    Ok(doc)
}

struct JsonPatch;

#[pg_aggregate]
impl Aggregate for JsonPatch {
    const NAME: &'static str = "json_patch_agg";
    const INITIAL_CONDITION: Option<&'static str> = Some("{}");

    type State = pgrx::Json;
    type Args = pgrx::name!(value, pgrx::Json);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        mut current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        // TODO: avoid clone?
        if let Ok(patch) = serde_json::from_value::<json_patch::Patch>(arg.0.clone()) {
            json_patch::patch(&mut current.0, &patch).unwrap_or_report();
        } else {
            json_patch::merge(&mut current.0, &arg.0);
        }
        current
    }
}

struct JsonBPatch;

#[pg_aggregate]
impl Aggregate for JsonBPatch {
    const NAME: &'static str = "jsonb_patch_agg";
    const INITIAL_CONDITION: Option<&'static str> = Some("{}");

    type State = pgrx::JsonB;
    type Args = pgrx::name!(value, pgrx::JsonB);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        mut current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        // TODO: avoid clone?
        if let Ok(patch) = serde_json::from_value::<json_patch::Patch>(arg.0.clone()) {
            json_patch::patch(&mut current.0, &patch).unwrap_or_report();
        } else {
            json_patch::merge(&mut current.0, &arg.0);
        }
        current
    }
}

struct JsonMerge;

#[pg_aggregate]
impl Aggregate for JsonMerge {
    const NAME: &'static str = "json_merge_agg";
    const INITIAL_CONDITION: Option<&'static str> = Some("{}");

    type State = pgrx::Json;
    type Args = pgrx::name!(value, pgrx::Json);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        mut current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        json_patch::merge(&mut current.0, &arg.0);
        current
    }
}

struct JsonBMerge;

#[pg_aggregate]
impl Aggregate for JsonBMerge {
    const NAME: &'static str = "jsonb_merge_agg";
    const INITIAL_CONDITION: Option<&'static str> = Some("{}");

    type State = pgrx::JsonB;
    type Args = pgrx::name!(value, pgrx::JsonB);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        mut current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        json_patch::merge(&mut current.0, &arg.0);
        current
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
pub(crate) mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_jsonb_patch() -> Result<(), spi::Error> {
        assert_eq!(
            Some(serde_json::json!({ "username": "three" })),
            Spi::get_one_with_args::<pgrx::JsonB>(
                "select jsonb_patch('{}', $1);",
                vec![(
                    PgBuiltInOids::JSONBOID.oid(),
                    pgrx::JsonB(serde_json::json!([
                        { "op": "add", "path": "/username", "value": "one" },
                        { "op": "replace", "path": "/username", "value": "two" },
                        { "op": "replace", "path": "/username", "value": "three" },
                    ]))
                    .into_datum()
                )]
            )?
            .map(|json| json.0)
        );

        Ok(())
    }
}
