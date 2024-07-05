use pgrx::prelude::*;

#[pg_extern]
fn html_minify(mut html: String) -> Result<String, minify_html_onepass::Error> {
    Ok(minify_html_onepass::in_place_str(
        &mut html,
        &minify_html_onepass::Cfg {
            minify_js: true,
            minify_css: true,
        },
    )?
    .to_string())
}

// TODO: check if shmem can be used to store named templates
#[pg_extern]
fn jinja_render(src: &str, ctx: pgrx::JsonB) -> Result<String, minijinja::Error> {
    minijinja::Environment::new().render_str(src, ctx.0)
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
pub(crate) mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn jinja_render_hello() -> Result<(), spi::Error> {
        assert_eq!(
            Spi::get_one_with_args(
                "select jinja_render($1, $2);",
                vec![
                    (
                        PgBuiltInOids::TEXTOID.oid(),
                        "hello {{ name }}".into_datum()
                    ),
                    (
                        PgBuiltInOids::JSONBOID.oid(),
                        pgrx::JsonB(serde_json::json!({ "name": "foo" })).into_datum()
                    )
                ]
            )?,
            Some("hello foo")
        );

        Ok(())
    }

    #[pg_test]
    fn jinja_render_table() -> Result<(), spi::Error> {
        assert_eq!(
            Spi::get_one_with_args(
                "select html_minify(jinja_render($1, $2));",
                vec![
                    (
                        PgBuiltInOids::TEXTOID.oid(),
                        include_str!("../../tmpl/table.html").into_datum()
                    ),
                    (
                        PgBuiltInOids::JSONBOID.oid(),
                        pgrx::JsonB(serde_json::json!({
                            "head": ["username", "email"],
                            "body": [
                                { "key": 1, "cols": ["one", "foo"] },
                                { "key": 2, "cols": ["two", "foo"] }
                            ]
                        }))
                        .into_datum()
                    )
                ]
            )?,
            Some(
                "<table><thead><tr><th scope=col>username<th scope=col>email<tbody><tr id=1><td>one<td>foo<tr id=2><td>two<td>foo</table>"
            )
        );

        Ok(())
    }

    #[pg_test]
    fn jinja_render_users() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;
        Spi::run("insert into users (id, username, salt, passhash, email) values ('00000000-0000-0000-0000-000000000001', 'one', '', '', 'foo'), ('00000000-0000-0000-0000-000000000002', 'two', '', '', 'foo');")?;

        assert_eq!(
            Spi::get_one_with_args(
                "select html_minify(jinja_render($1, (select jsonb_build_object('head', array['username', 'email'], 'body', (select array_agg(jsonb_build_object('key', id, 'cols', array[username, email])) from users)))));",
                vec![(PgBuiltInOids::TEXTOID.oid(), include_str!("../../tmpl/table.html").into_datum())]
            )?,
            Some("<table><thead><tr><th scope=col>username<th scope=col>email<tbody><tr id=00000000-0000-0000-0000-000000000001><td>one<td>foo<tr id=00000000-0000-0000-0000-000000000002><td>two<td>foo</table>")
        );

        Ok(())
    }

    #[pg_test]
    fn jinja_render_empty() -> Result<(), spi::Error> {
        Spi::run(include_str!("../sql/users.sql"))?;

        assert_eq!(
            Spi::get_one_with_args(
                "select html_minify(jinja_render($1, (select jsonb_build_object('head', array['username', 'email'], 'body', (select array_agg(jsonb_build_object('key', id, 'cols', array[username, email])) from users)))));",
                vec![(PgBuiltInOids::TEXTOID.oid(), include_str!("../../tmpl/table.html").into_datum())]
            )?,
            Some("")
        );

        Ok(())
    }
}
