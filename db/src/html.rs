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
