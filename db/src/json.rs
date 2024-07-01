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

struct JsonPatch;

#[pg_aggregate]
impl Aggregate for JsonPatch {
    const NAME: &'static str = "json_patch";
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
    const NAME: &'static str = "jsonb_patch";
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
    const NAME: &'static str = "json_merge";
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
    const NAME: &'static str = "jsonb_merge";
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
