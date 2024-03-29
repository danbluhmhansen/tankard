use pgrx::prelude::*;

::pgrx::pg_module_magic!();

struct JsonMerge;

#[pg_aggregate]
impl Aggregate for JsonMerge {
    const NAME: &'static str = "json_merge";
    const INITIAL_CONDITION: Option<&'static str> = Some("{}");

    type State = pgrx::Json;
    type Args = pgrx::Json;

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

extension_sql_file!("../migrations/20240329145141.sql");
extension_sql_file!("../sql/users.sql", requires = ["20240329145141"]);
extension_sql_file!("../sql/games.sql", requires = ["users"]);

#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_init_users() {
        _ = Spi::run("create extension if not exists pgcrypto;");
        let (id, name) = Spi::get_two::<pgrx::Uuid, &str>(
            r#"select stream_id, data ->> 'username' from init_users('[{"username":"foo","password":"bar"}]'::jsonb);"#,
        ).expect("user initialized successfully");
        assert_eq!(Some("foo"), name);
        assert_eq!(Ok(id), Spi::get_one("select id from user_streams;"));
        assert_eq!(Ok(Some("foo")), Spi::get_one("select username from users;"));
    }

    #[pg_test]
    fn test_set_users() {
        _ = Spi::run("create extension if not exists pgcrypto;");
        let id = Spi::get_one::<pgrx::Uuid>(
            r#"select stream_id from init_users('[{"username":"foo","password":"bar"}]'::jsonb);"#,
        )
        .ok()
        .flatten()
        .expect("user initialized successfully");
        assert_eq!(
            Ok(Some("another")),
            Spi::get_one(format!(
                r#"select data ->> 'username' from set_users('[{{"id":"{id}","username":"another"}}]'::jsonb);"#
            ).as_str())
        );
        assert_eq!(
            Ok(Some("another")),
            Spi::get_one("select username from users;")
        );
    }

    #[pg_test]
    fn test_drop_users() {
        _ = Spi::run("create extension if not exists pgcrypto;");
        let id = Spi::get_one::<pgrx::Uuid>(
            r#"select stream_id from init_users('[{"username":"foo","password":"bar"}]'::jsonb);"#,
        )
        .ok()
        .flatten()
        .expect("user initialized successfully");
        assert_eq!(
            Ok(Some("dropped")),
            Spi::get_one(
                format!(r#"select name from drop_users(array['{id}']::uuid[]);"#).as_str()
            )
        );
        assert_eq!(
            Ok(Some(0)),
            Spi::get_one::<i64>("select count(*) from users;")
        );
    }

    #[pg_test]
    fn test_restore_users() {
        _ = Spi::run("create extension if not exists pgcrypto;");
        let id = Spi::get_one::<pgrx::Uuid>(
            r#"select stream_id from init_users('[{"username":"foo","password":"bar"}]'::jsonb);"#,
        )
        .ok()
        .flatten()
        .expect("user initialized successfully");
        _ = Spi::run(format!(r#"select name from drop_users(array['{id}']::uuid[]);"#).as_str());
        assert_eq!(
            Ok(Some("restored")),
            Spi::get_one(
                format!(r#"select name from restore_users(array['{id}']::uuid[]);"#).as_str()
            )
        );
        assert_eq!(Ok(Some("foo")), Spi::get_one("select username from users;"));
    }

    #[pg_test]
    fn test_snap_users() {
        _ = Spi::run("create extension if not exists pgcrypto;");
        let id = Spi::get_one::<pgrx::Uuid>(
            r#"select stream_id from init_users('[{"username":"foo","password":"bar"}]'::jsonb);"#,
        )
        .ok()
        .flatten()
        .expect("user initialized successfully");
        _ = Spi::run(
            format!(r#"select data from set_users('[{{"id":"{id}","email":"email"}}]'::jsonb);"#)
                .as_str(),
        );
        assert_eq!(
            Ok((Some("snap"), Some("foo"), Some("email"))),
            Spi::get_three("select name, data ->> 'username', data ->> 'email' from snap_users(clock_timestamp());")
        );
    }

    #[pg_test]
    fn test_check_password() {
        _ = Spi::run("create extension if not exists pgcrypto;");
        let id = Spi::get_one::<pgrx::Uuid>(
            r#"select stream_id from init_users('[{"username":"foo","password":"bar"}]'::jsonb);"#,
        )
        .ok()
        .flatten()
        .expect("user initialized successfully");
        assert_eq!(
            Ok(Some(true)),
            Spi::get_one(format!("select check_password('{id}','bar');").as_str())
        );
    }

    #[pg_test]
    fn test_init_games() {
        _ = Spi::run("create extension if not exists pgcrypto;");
        let user_id = Spi::get_one::<pgrx::Uuid>(
            r#"select stream_id from init_users('[{"username":"foo","password":"bar"}]'::jsonb);"#,
        )
        .ok()
        .flatten()
        .expect("user initialized successfully");
        let (id, name) = Spi::get_two::<pgrx::Uuid, &str>(
            format!(r#"select stream_id, data ->> 'name' from init_games('[{{"user_id":"{user_id}","name":"foo"}}]'::jsonb);"#)
                .as_str()
        )
        .expect("game initialized successfully");
        assert_eq!(Some("foo"), name);
        assert_eq!(Ok(id), Spi::get_one("select id from game_streams;"));
        assert_eq!(Ok(Some("foo")), Spi::get_one("select name from games;"));
    }
}

/// This module is required by `cargo pgrx test` invocations.
/// It must be visible at the root of your extension crate.
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {
        // perform one-off initialization when the pg_test framework starts
    }

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        // return any postgresql.conf settings that are required for your tests
        vec![]
    }
}
