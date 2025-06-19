#[macro_use]
extern crate rocket;

mod paste_id;
mod os_header;
mod highlight;
mod cors;
mod config;
mod reaper;

use rocket::tokio::{fs, io};
use rocket_dyn_templates::{Template, context};

use rocket::State;
use rocket::form::Form;
use rocket::data::Capped;
use rocket::fairing::AdHoc;
use rocket::response::Redirect;
use rocket::request::FlashMessage;
use rocket::fs::{FileServer, TempFile};
use rocket::http::{Status, ContentType};

use paste_id::PasteId;
use os_header::ClientOs;
use highlight::{Highlighter, HIGHLIGHT_EXTS};
use config::Config;
use cors::Cors;
use reaper::Reaper;

#[derive(Responder)]
pub enum Paste {
    Highlighted(Template),
    Regular(Vec<u8>, ContentType),
    Markdown(Template),
    Asciinema(Template),
}

#[derive(Debug, FromForm)]
struct PasteForm<'r> {
    #[field(validate = len(1..))]
    content: Capped<TempFile<'r>>,
    #[field(validate = with(|e| Highlighter::contains(e), "unknown extension"))]
    ext: &'r str,
}

#[post("/", data = "<paste>")]
async fn upload(
    mut paste: Capped<TempFile<'_>>,
    config: &Config,
) -> io::Result<(Status, String)> {
    let id = PasteId::new(config);
    paste.persist_to(id.file_path(config)).await?;

    let paste_uri = uri!(config.server_url.clone(), get(id));
    let status = match paste.is_complete() {
        true => Status::Created,
        false => Status::PartialContent,
    };

    Ok((status, paste_uri.to_string()))
}

#[post("/web", data = "<form>")]
async fn web_form_submit(
    mut form: Form<PasteForm<'_>>,
    config: &Config,
) -> io::Result<Redirect> {
    let id = PasteId::with_ext(config, form.ext);
    form.content.persist_to(&id.file_path(config)).await?;
    Ok(Redirect::to(uri!(get(id))))
}

// TODO: Authenticate a delete using some kind of token.
#[delete("/<id>")]
async fn delete(id: PasteId<'_>, config: &Config) -> Option<&'static str> {
    fs::remove_file(&id.file_path(config)).await.map(|_| "deleted\n").ok()
}

#[get("/<id>")]
async fn get(
    id: PasteId<'_>,
    highlighter: &State<Highlighter>,
    config: &Config,
) -> io::Result<Option<Paste>> {
    let path = id.file_path(config);
    if !path.exists() {
        return Ok(None);
    }

    let data = fs::read(path).await?;
    let paste = match id.ext() {
        Some("md" | "mdown" | "markdown") => {
            let string = String::from_utf8_lossy(&data);
            let content = highlighter.render_markdown(&string)?;
            Paste::Markdown(Template::render("markdown", context! { config, id, content }))
        }
        Some("cast") => {
            let base = id.base();
            Paste::Asciinema(Template::render("asciinema", context! { config, id, base }))
        }
        Some(ext) if Highlighter::contains(ext) => {
            let string = String::from_utf8_lossy(&data);
            let content = highlighter.highlight(&string, ext)?;
            let lines = content.lines().count();
            Paste::Highlighted(Template::render("code", context! { config, id, content, lines }))
        }
        _ => Paste::Regular(data, id.content_type().unwrap_or(ContentType::Plain)),
    };

    Ok(Some(paste))
}

#[get("/")]
fn index(config: &Config, os: Option<ClientOs>) -> Template {
    let os = match os.map(|o| o.name()) {
        Some(os @ ("windows" | "linux" | "darwin")) => os,
        _ => "unix",
    };

    let cmd = match (os, &*config.example_socks5_addr) {
        ("windows", "") => "PowerShell",
        ("windows", _) => "PowerShell (v6+)",
        _ => "cURL",
    };

    let mut flag = String::new();
    if !config.example_socks5_addr.is_empty() {
        if os == "windows" {
            flag.push_str(" -Proxy socks5://");
        } else {
            flag.push_str(" --socks5-hostname ");
        }
        flag.push_str(&config.example_socks5_addr);
    };
    Template::render("index", context! { config, cmd, os, flag })
}

#[get("/web")]
fn web_form(config: &Config, flash: Option<FlashMessage>) -> Template {
    Template::render("new", context! {
        config,
        extensions: HIGHLIGHT_EXTS,
        error: flash.as_ref().map(FlashMessage::message),
    })
}

#[rocket::launch]
fn rocket() -> _ {
    rocket::custom(Config::figment())
        .mount("/", routes![index, upload, get, delete, web_form, web_form_submit])
        .mount("/", FileServer::from("static").rank(-20))
        .manage(Highlighter::default().expect("failed to load syntax highlighter"))
        .attach(Template::fairing())
        .attach(AdHoc::config::<Config>())
        .attach(Reaper::fairing())
        .attach(Cors::fairing())
}
