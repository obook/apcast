//! info.rs
//! First exchange with a receiver: `GET /info` returns a binary plist with its name, model,
//! version, screen and status flags. `apcast info` prints it; a cast logs it.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: MIT

use std::fmt;
use std::io::Cursor;

use anyhow::Context;
use plist::Value;

use super::rtsp::Rtsp;

/// Status flag bits, from the unofficial protocol specification; unverified on an Apple TV.
const FLAGS: [(u64, &str); 3] = [(0x8, "PIN required"), (0x80, "password required"), (0x200, "one-time pairing required")];

/// What the receiver says about itself. A field the answer leaves out stays None.
#[derive(Debug, Default, PartialEq)]
pub struct Info {
    pub name: Option<String>,
    pub model: Option<String>,
    pub source_version: Option<String>,
    pub features: Option<u64>,
    pub status_flags: Option<u64>,
    /// Width and height of the receiver's first display.
    pub screen: Option<(u64, u64)>,
}

/// Ask the receiver for its info. Returns the whole plist too, for `apcast info --raw`.
pub fn fetch(rtsp: &mut Rtsp) -> anyhow::Result<(Info, Value)> {
    let body = rtsp.ok("GET", "/info", "", &[])?;
    let plist = Value::from_reader(Cursor::new(body)).context("the info answer is not a plist")?;
    Ok((Info::from_plist(&plist)?, plist))
}

impl Info {
    fn from_plist(plist: &Value) -> anyhow::Result<Self> {
        let dict = plist.as_dictionary().context("the info answer is not a dictionary")?;
        let text = |key| dict.get(key).and_then(Value::as_string).map(str::to_string);
        let number = |key| dict.get(key).and_then(Value::as_unsigned_integer);
        let first_display = dict.get("displays").and_then(Value::as_array).and_then(|d| d.first()).and_then(Value::as_dictionary);
        let screen = first_display.and_then(|d| Some((d.get("width")?.as_unsigned_integer()?, d.get("height")?.as_unsigned_integer()?)));
        Ok(Self {
            name: text("name"),
            model: text("model"),
            source_version: text("sourceVersion"),
            features: number("features"),
            status_flags: number("statusFlags"),
            screen,
        })
    }

    /// Access the status flags ask for, by name. Empty when none, or no flags given.
    pub fn access(&self) -> Vec<&'static str> {
        let flags = self.status_flags.unwrap_or(0);
        FLAGS.iter().filter(|(bit, _)| flags & bit != 0).map(|(_, name)| *name).collect()
    }
}

impl fmt::Display for Info {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let show = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
        writeln!(f, "name\t{}", show(&self.name))?;
        writeln!(f, "model\t{}", show(&self.model))?;
        writeln!(f, "version\t{}", show(&self.source_version))?;
        writeln!(f, "screen\t{}", self.screen.map_or("-".into(), |(w, h)| format!("{w}x{h}")))?;
        writeln!(f, "features\t{}", self.features.map_or("-".into(), |v| format!("{v:#x}")))?;
        writeln!(f, "flags\t{}", self.status_flags.map_or("-".into(), |v| format!("{v:#x}")))?;
        let access = self.access();
        write!(f, "access\t{}", if access.is_empty() { "open".to_string() } else { access.join(", ") })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_info_answer() {
        let mut display = plist::Dictionary::new();
        display.insert("width".into(), 1920.into());
        display.insert("height".into(), 1080.into());
        let mut dict = plist::Dictionary::new();
        dict.insert("name".into(), "Salle 104".into());
        dict.insert("model".into(), "AppleTV6,2".into());
        dict.insert("statusFlags".into(), 0x84.into());
        dict.insert("displays".into(), Value::Array(vec![display.into()]));
        let mut body = Vec::new();
        plist::to_writer_binary(&mut body, &Value::from(dict)).unwrap();

        let info = Info::from_plist(&Value::from_reader(Cursor::new(body)).unwrap()).unwrap();
        assert_eq!(info.name.as_deref(), Some("Salle 104"));
        assert_eq!(info.screen, Some((1920, 1080)));
        assert_eq!(info.access(), ["password required"]);
    }
}
