#[macro_use]
extern crate rocket;

mod config;
mod cors;
mod highlight;
mod paste_id;
mod reaper;

use rocket::tokio::fs;
use rocket_dyn_templates::{Template, context};

use rocket::State;
use rocket::data::Capped;
use rocket::fairing::AdHoc;
use rocket::form::Form;
use rocket::fs::{FileServer, TempFile};
use rocket::http::{ContentType, Status};
use rocket::request::FlashMessage;
use rocket::response::Redirect;

use config::Config;
use cors::Cors;
use highlight::{HIGHLIGHT_EXTS, Highlighter};
use paste_id::PasteId;
use reaper::Reaper;

#[derive(thiserror::Error, Debug, Responder)]
pub(crate) enum Error {
    #[error("input/output")]
    Io(#[from] std::io::Error),
}

impl From<&'static str> for Error {
    fn from(other: &'static str) -> Self {
        Self::Io(std::io::Error::other(other))
    }
}

impl From<syntect::Error> for Error {
    fn from(other: syntect::Error) -> Self {
        Self::Io(std::io::Error::other(other.to_string()))
    }
}

impl From<std::fmt::Error> for Error {
    fn from(other: std::fmt::Error) -> Self {
        Self::Io(std::io::Error::other(other.to_string()))
    }
}

pub(crate) type Result<T> = core::result::Result<T, Error>;

#[derive(Responder)]
pub enum Paste {
    Highlighted(Template),
    Regular(String, ContentType),
    Markdown(Template),
}

#[derive(Debug, FromForm)]
struct PasteForm<'r> {
    #[field(validate = len(1..))]
    content: Capped<TempFile<'r>>,
    #[field(validate = with(|e| Highlighter::contains(e), "unknown extension"))]
    ext: &'r str,
}

#[post("/", data = "<paste>")]
async fn upload(mut paste: Capped<TempFile<'_>>, config: &Config) -> Result<(Status, String)> {
    let id = PasteId::new(config);
    paste.persist_to(id.file_path(config)).await?;

    let paste_uri = uri!(config.server_url.clone(), get(id));
    let status = if paste.is_complete() {
        Status::Created
    } else {
        Status::PartialContent
    };

    Ok((status, paste_uri.to_string()))
}

#[post("/web", data = "<form>")]
async fn web_form_submit(mut form: Form<PasteForm<'_>>, config: &Config) -> Result<Redirect> {
    let id = PasteId::with_ext(config, form.ext);
    form.content.persist_to(&id.file_path(config)).await?;
    Ok(Redirect::to(uri!(get(id))))
}

#[post("/<ext>", data = "<paste>")]
async fn upload_ext(
    ext: &str,
    mut paste: Capped<TempFile<'_>>,
    config: &Config,
) -> Result<(Status, String)> {
    let id = PasteId::with_ext(config, ext);
    paste.persist_to(id.file_path(config)).await?;

    let paste_uri = uri!(config.server_url.clone(), get(id));
    let status = if paste.is_complete() {
        Status::Created
    } else {
        Status::PartialContent
    };

    Ok((status, paste_uri.to_string()))
}

// TODO: Authenticate a delete using some kind of token.
#[delete("/<id>")]
async fn delete(id: PasteId<'_>, config: &Config) -> Option<&'static str> {
    fs::remove_file(&id.file_path(config))
        .await
        .and(Ok("deleted\n"))
        .ok()
}

#[get("/raw/<id>")]
async fn get_raw(id: PasteId<'_>, config: &Config) -> Result<Option<Paste>> {
    let path = id.file_path(config);
    if !path.exists() {
        return Ok(None);
    }

    Ok(Some(Paste::Regular(
        fs::read_to_string(path).await?,
        ContentType::Plain,
    )))
}

#[get("/theme.css")]
async fn get_theme(highlighter: &State<Highlighter>) -> Result<Option<Paste>> {
    Ok(Some(Paste::Regular(
        highlighter.style().to_owned(),
        ContentType::CSS,
    )))
}

#[get("/<id>")]
async fn get(
    id: PasteId<'_>,
    highlighter: &State<Highlighter>,
    config: &Config,
) -> Result<Option<Paste>> {
    let path = id.file_path(config);
    if !path.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(path).await?;
    let paste = match id.ext {
        Some("md" | "mdown" | "markdown") => {
            let content = highlighter.render_markdown(&data)?;
            Paste::Markdown(Template::render(
                "markdown",
                context! { config, id, content },
            ))
        }
        Some(ext) if Highlighter::contains(ext) => {
            let content = highlighter.highlight(&data, ext)?;
            let lines = content.lines().count();
            Paste::Highlighted(Template::render(
                "code",
                context! { config, id, content, lines },
            ))
        }
        _ => {
            let lines = data.lines().count();
            Paste::Highlighted(Template::render(
                "plain",
                context! { config, id, content: data, lines },
            ))
        }
    };

    Ok(Some(paste))
}

#[get("/")]
fn index(config: &Config) -> Template {
    Template::render("index", context! { config })
}

#[get("/web")]
fn web_form(config: &Config, flash: Option<FlashMessage>) -> Template {
    Template::render(
        "new",
        context! {
            config,
            extensions: &*HIGHLIGHT_EXTS,
            error: flash.map(|f| f.into_inner().1),
        },
    )
}

#[rocket::launch]
fn rocket() -> _ {
    rocket::custom(Config::figment())
        .mount(
            "/",
            routes![
                index,
                upload,
                get,
                get_raw,
                get_theme,
                delete,
                web_form,
                web_form_submit,
                upload_ext
            ],
        )
        .mount("/", FileServer::from("static").rank(-20))
        .manage(Highlighter::default().expect("failed to load syntax highlighter"))
        .attach(Template::fairing())
        .attach(AdHoc::config::<Config>())
        .attach(Reaper::fairing())
        .attach(Cors::fairing())
}
