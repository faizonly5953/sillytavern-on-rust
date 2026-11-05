//! `avatar://` URI scheme serving character/persona images (with thumbnails).
//!
//! JS builds URLs with `convertFileSrc("character/<file>", "avatar")`
//! (`http://avatar.localhost/...` on Windows/Android). `?thumb=1` returns a small
//! cached PNG for lists.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::error::{AppError, AppResult};
use crate::paths;
use crate::state::{AppState, lock};

pub fn handle<R: Runtime>(ctx: UriSchemeContext<'_, R>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = match app.try_state::<AppState>() {
            Some(state) => serve(&state, &request),
            None => Err(StatusCode::SERVICE_UNAVAILABLE),
        };
        let response = result.unwrap_or_else(|status| {
            Response::builder().status(status).body(Vec::new()).expect("empty response is valid")
        });
        responder.respond(response);
    });
}

fn serve(state: &AppState, request: &Request<Vec<u8>>) -> Result<Response<Vec<u8>>, StatusCode> {
    let path = percent_decode(request.uri().path().trim_start_matches('/'));
    let (kind, file) = path.split_once('/').ok_or(StatusCode::BAD_REQUEST)?;
    let dir = match kind {
        "character" => state.dirs.characters(),
        "persona" => state.dirs.user_avatars(),
        _ => return Err(StatusCode::NOT_FOUND),
    };
    let full = dir.join(paths::check_file_name(file).map_err(|_| StatusCode::BAD_REQUEST)?);
    let thumb = request.uri().query().is_some_and(|q| q.split('&').any(|p| p == "thumb=1"));
    let body = if thumb { thumbnail(state, &full) } else { fs::read(&full).map_err(AppError::from) }
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn thumbnail(state: &AppState, path: &Path) -> AppResult<Vec<u8>> {
    let modified = paths::modified_ms(path);
    if let Some((stamp, bytes)) = lock(&state.thumbnails).get(path)
        && *stamp == modified
    {
        return Ok(bytes.clone());
    }
    let img = image::load_from_memory(&fs::read(path)?)?;
    let mut out = Vec::new();
    img.thumbnail(192, 288).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)?;
    lock(&state.thumbnails).insert(path.to_path_buf(), (modified, out.clone()));
    Ok(out)
}

fn percent_decode(input: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(high << 4 | low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn decodes_paths() {
        assert_eq!(percent_decode("character%2FSera%20phina.png"), "character/Sera phina.png");
        assert_eq!(percent_decode("persona%2F%E2%9C%93.png"), "persona/\u{2713}.png");
        assert_eq!(percent_decode("bad%zzend%2"), "bad%zzend%2");
    }
}
