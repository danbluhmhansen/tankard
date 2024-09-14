use std::{error::Error, fs, io::Write};

fn main() -> Result<(), Box<dyn Error>> {
    _ = fs::create_dir("dist");

    let pico_path = "dist/simple.css";
    if fs::metadata(pico_path).is_err() {
        let url = "https://github.com/kevquirk/simple.css/raw/v2.3.2/simple.css";
        let response = ureq::get(url).call()?;
        fs::File::create(pico_path)?.write_all(response.into_string()?.as_ref())?;
    }

    let htmx_path = "dist/htmx.js";
    if fs::metadata(htmx_path).is_err() {
        let url = "https://github.com/bigskysoftware/htmx/raw/v2.0.2/dist/htmx.js";
        let response = ureq::get(url).call()?;
        fs::File::create(htmx_path)?.write_all(response.into_string()?.as_ref())?;
    }

    let sse_path = "dist/sse.js";
    if fs::metadata(sse_path).is_err() {
        let url = "https://github.com/bigskysoftware/htmx-extensions/raw/c328002fd124c7a6745cf8a302f163d1e291cc3f/src/sse/sse.js";
        let response = ureq::get(url).call()?;
        fs::File::create(sse_path)?.write_all(response.into_string()?.as_ref())?;
    }

    Ok(())
}
