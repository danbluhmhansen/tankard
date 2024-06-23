use pgrx::prelude::*;

struct First;

#[pg_aggregate]
impl Aggregate for First {
    type State = pgrx::AnyElement;
    type Args = pgrx::name!(value, pgrx::AnyElement);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        current: Self::State,
        _arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        current
    }
}

struct Last;

#[pg_aggregate]
impl Aggregate for Last {
    type State = pgrx::AnyElement;
    type Args = pgrx::name!(value, pgrx::AnyElement);

    #[pgrx(parallel_safe, immutable, strict)]
    fn state(
        _current: Self::State,
        arg: Self::Args,
        _fcinfo: pg_sys::FunctionCallInfo,
    ) -> Self::State {
        arg
    }
}
