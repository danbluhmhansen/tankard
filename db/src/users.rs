#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
pub(crate) mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn users_insert() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        let (user_id, user_ts, username) = Spi::get_three::<pgrx::Uuid, pgrx::TimestampWithTimeZone, &str>(
            "insert into users (username, salt, passhash) values ('foo', '', '') returning id, updated, username;",
        )?;

        let (stream_id, stream_user_id) =
            Spi::get_two::<pgrx::Uuid, pgrx::Uuid>("select id, users_id from users_streams;")?;

        assert_eq!(
            user_id, stream_user_id,
            "generated user id should match the stream id"
        );

        let (event_ts, event_stream_id, event_data) =
            Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::Uuid, pgrx::JsonB>(
                "select timestamp, stream_id, data from users_events;",
            )?;

        assert_eq!(
            user_ts, event_ts,
            "added user timestamp should match event timestamp"
        );

        assert_eq!(
            stream_id, event_stream_id,
            "stream id should match the event's stream id"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": username },
            ])),
            event_data.map(|data| data.0),
            "added user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_update() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        Spi::run("insert into users (username, salt, passhash) values ('foo', '', '');")?;
        let (user_ts, username) = Spi::get_two::<pgrx::TimestampWithTimeZone, &str>(
            "update users set username = 'bar' returning updated, username;",
        )?;

        let (event_ts, event_data) = Spi::get_two::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select timestamp, data from users_events offset 1;",
        )?;

        assert_eq!(
            user_ts, event_ts,
            "updated user timestamp should match event timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "replace", "path": "/username", "value": username },
            ])),
            event_data.map(|data| data.0),
            "updated user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_update_set_null() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        Spi::run("insert into users (username, salt, passhash, email) values ('foo', '', '', 'foo@bar.com');")?;
        let user_ts = Spi::get_one::<pgrx::TimestampWithTimeZone>(
            "update users set email = null returning updated;",
        )?;

        let (event_ts, event_data) = Spi::get_two::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select timestamp, data from users_events offset 1;",
        )?;

        assert_eq!(
            user_ts, event_ts,
            "updated user timestamp should match event timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "remove", "path": "/email" },
            ])),
            event_data.map(|data| data.0),
            "updated user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_delete() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        Spi::run("insert into users (username, salt, passhash) values ('foo', '', '');")?;
        Spi::run("delete from users;")?;

        let event_data = Spi::get_one::<pgrx::JsonB>("select data from users_events offset 1;")?;

        assert_eq!(
            Some(serde_json::json!([{"op": "replace", "path": "", "value": null}])),
            event_data.map(|data| data.0),
            "delete event should indicate the user has been deleted"
        );

        Ok(())
    }

    #[pg_test]
    fn users_ts() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        let insert =
            Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning added, updated, username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let ts = Spi::get_three_with_args::<
            pgrx::TimestampWithTimeZone,
            pgrx::TimestampWithTimeZone,
            &str,
        >(
            "select added, updated, username from users_ts($1);",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), insert.0.into_datum())],
        )?;

        assert_eq!(
            insert, ts,
            "user fetched from insert timestamp should match originally inserted user"
        );

        Ok(())
    }

    #[pg_test]
    fn users_ts_deleted() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        let insert =
            Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning added, updated, username;"
            )?;
        Spi::run("delete from users;")?;

        let ts = Spi::get_three_with_args::<
            pgrx::TimestampWithTimeZone,
            pgrx::TimestampWithTimeZone,
            &str,
        >(
            "select added, updated, username from users_ts($1);",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), insert.0.into_datum())],
        )?;

        assert_eq!(
            insert, ts,
            "user fetched from insert timestamp should match originally inserted user"
        );

        Ok(())
    }

    #[pg_test]
    fn users_step() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        let insert =
            Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning added, updated, username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let ts = Spi::get_three::<pgrx::TimestampWithTimeZone, pgrx::TimestampWithTimeZone, &str>(
            "select added, updated, username from users_step();",
        )?;

        assert_eq!(
            insert, ts,
            "user fetched from insert timestamp should match originally inserted user"
        );

        Ok(())
    }

    #[pg_test]
    fn users_ts_commit() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        let (ts, username) =
            Spi::get_two::<pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning updated, username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let updated = Spi::get_one_with_args::<pgrx::TimestampWithTimeZone>(
            "select (users_commit).updated from (select users_commit(array_agg) from (select array_agg(users_ts) from users_ts($1)));",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())],
        )?;

        let (event_ts, event_data) = Spi::get_two::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select timestamp, data from users_events offset 2;",
        )?;

        assert_eq!(
            updated, event_ts,
            "commited user timestamp should match event timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "replace", "path": "/username", "value": username },
            ])),
            event_data.map(|data| data.0),
            "commited user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_ts_commit_deleted() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        let (ts, username) =
            Spi::get_two::<pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning updated, username;"
            )?;
        Spi::run("delete from users;")?;

        let updated = Spi::get_one_with_args::<pgrx::TimestampWithTimeZone>(
            "select (users_commit).updated from (select users_commit(array_agg) from (select array_agg(users_ts) from users_ts($1)));",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())],
        )?;

        let (event_ts, event_data) = Spi::get_two::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select timestamp, data from users_events offset 2;",
        )?;

        assert_eq!(
            updated, event_ts,
            "commited user timestamp should match event timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": username },
            ])),
            event_data.map(|data| data.0),
            "commited user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_step_commit() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        let username =
            Spi::get_one::<&str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let updated = Spi::get_one::<pgrx::TimestampWithTimeZone>(
            "select (users_commit).updated from (select users_commit(array_agg) from (select array_agg(users_step) from users_step()));",
        )?;

        let (event_ts, event_data) = Spi::get_two::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select timestamp, data from users_events offset 2;",
        )?;

        assert_eq!(
            updated, event_ts,
            "commited user timestamp should match event timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "replace", "path": "/username", "value": username },
            ])),
            event_data.map(|data| data.0),
            "commited user data should match the json patch data in the event"
        );

        Ok(())
    }

    #[pg_test]
    fn users_snaps() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("select init_event_source('users');")?;

        let (ts, username) =
            Spi::get_two::<pgrx::TimestampWithTimeZone, &str>(
                "insert into users (username, salt, passhash) values ('foo', '', '') returning updated, username;"
            )?;
        Spi::run("update users set username = 'bar';")?;

        let (snap_ts, snap_data) = Spi::get_two_with_args::<pgrx::TimestampWithTimeZone, pgrx::JsonB>(
            "select (users_snaps_commit).timestamp, (users_snaps_commit).data from (select users_snaps_commit(array_agg) from (select array_agg(users_ts) from users_ts($1)));",
            vec![(PgBuiltInOids::TIMESTAMPTZOID.oid(), ts.into_datum())],
        )?;

        assert_eq!(
            ts, snap_ts,
            "snapped user timestamp should match snap timestamp"
        );

        assert_eq!(
            Some(serde_json::json!([
                { "op": "add", "path": "/passhash", "value": "" },
                { "op": "add", "path": "/salt", "value": "" },
                { "op": "add", "path": "/username", "value": username },
            ])),
            snap_data.map(|data| data.0),
            "snapped user data should match the json patch data in the snap"
        );

        Ok(())
    }
}