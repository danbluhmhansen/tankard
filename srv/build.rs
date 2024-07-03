use std::{error::Error, fs, io::Write};

fn main() -> Result<(), Box<dyn Error>> {
    let pico_path = "dist/pico.css";
    if fs::metadata(pico_path).is_err() {
        let url = "https://github.com/picocss/pico/raw/v2.0.6/css/pico.css";
        let response = ureq::get(url).call()?;
        fs::File::create(pico_path)?.write_all(response.into_string()?.as_ref())?;
    }

    let htmx_path = "dist/htmx.js";
    if fs::metadata(htmx_path).is_err() {
        let url = "https://github.com/bigskysoftware/htmx/raw/v2.0.0/dist/htmx.js";
        let response = ureq::get(url).call()?;
        fs::File::create(htmx_path)?.write_all(response.into_string()?.as_ref())?;
    }

    let sse_path = "dist/sse.js";
    if fs::metadata(sse_path).is_err() {
        let url = "https://github.com/bigskysoftware/htmx-extensions/raw/d923e9416aa97cbb8f377cb11b1c2bf9a0afc012/src/sse/sse.js";
        let response = ureq::get(url).call()?;
        fs::File::create(sse_path)?.write_all(response.into_string()?.as_ref())?;
    }

    Ok(())
}
