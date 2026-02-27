use std::borrow::Cow;
use std::fmt;
use std::path::PathBuf;

use rand::distr::{Alphanumeric, SampleString};
use rand::rng;

use rocket::http::impl_from_uri_param_identity;
use rocket::http::uri::{self, fmt::UriDisplay};
use rocket::request::FromParam;
use rocket::serde::{Serialize, Serializer};

use crate::Config;

/// The (id, extension) of the requested paste.
pub struct PasteId<'a> {
    pub base: Cow<'a, str>,
    pub ext: Option<&'a str>,
}

impl<'a> PasteId<'a> {
    /// Generates a new, random paste ID.
    pub fn new(config: &Config) -> PasteId<'static> {
        PasteId::with_ext(config, None)
    }

    /// Randomly generates an ID of the configured length. There are no
    /// requirements on `ext`; it used simply as a hint in the responder.
    pub fn with_ext(config: &Config, ext: impl Into<Option<&'a str>>) -> Self {
        let id = Alphanumeric.sample_string(&mut rng(), config.id_length);
        PasteId {
            base: id.into(),
            ext: ext.into(),
        }
    }

    /// Where the paste with this ID should be stored.
    pub fn file_path(&self, config: &Config) -> PathBuf {
        config.upload_dir.relative().join(&*self.base)
    }
}

impl<'a> FromParam<'a> for PasteId<'a> {
    type Error = &'a str;

    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        let (base, ext) = param
            .rsplit_once('.')
            .map_or((param, None), |(a, b)| (a, Some(b)));
        if !base.chars().all(char::is_alphanumeric) {
            return Err(param);
        }

        Ok(PasteId {
            base: base.into(),
            ext: ext.filter(|e| !e.is_empty()),
        })
    }
}

impl Serialize for PasteId<'_> {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&self.base)
    }
}

impl UriDisplay<uri::fmt::Path> for PasteId<'_> {
    fn fmt(&self, f: &mut uri::fmt::Formatter<'_, uri::fmt::Path>) -> fmt::Result {
        self.base.fmt(f)?;
        if let Some(ext) = self.ext {
            f.write_raw(".")?;
            ext.fmt(f)?;
        }

        Ok(())
    }
}

impl_from_uri_param_identity!([uri::fmt::Path] ('a) PasteId<'a>);
