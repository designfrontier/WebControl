//! The user's tool library, `~/.WebControl/tools.json`: what each T number in their gcode is.

use std::io;
use std::path::PathBuf;

use serde_json::{Map, Value};
use wc_protocol::tools::{Tool, ToolLibrary, ToolUnits};

use crate::Paths;
use crate::file::{set_aside_corrupt, write_atomically};

pub const NUMBER_MAX: u64 = 99_999;
pub const DESCRIPTION_MAX: usize = 200;
pub const NOTES_MAX: usize = 1000;
pub const DIAMETER_MAX: f64 = 1000.0;
pub const FLUTES_MAX: u64 = 20;

/// Why a library write was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Invalid(String),
    Taken(u32),
    Missing(u32),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Invalid(why) => f.write_str(why),
            Refusal::Taken(number) => write!(f, "Your tool library already has a tool {number}."),
            Refusal::Missing(number) => write!(f, "Your tool library has no tool {number}."),
        }
    }
}

impl Paths {
    pub fn tools_file(&self) -> PathBuf {
        self.dir().join("tools.json")
    }
}

fn invalid(why: &str) -> Refusal {
    Refusal::Invalid(why.into())
}

fn optional<'a>(body: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    body.get(key).filter(|value| !value.is_null())
}

fn whole(value: &Value, max: u64, why: &str) -> Result<u32, Refusal> {
    let number = value.as_u64().filter(|n| (1..=max).contains(n)).ok_or_else(|| invalid(why))?;
    u32::try_from(number).map_err(|_| invalid(why))
}

fn text(body: &Map<String, Value>, key: &str, max: usize, why: &str) -> Result<String, Refusal> {
    match optional(body, key) {
        None => Ok(String::new()),
        Some(Value::String(text)) if text.trim().chars().count() <= max => Ok(text.trim().to_string()),
        Some(_) => Err(invalid(why)),
    }
}

/// A POST or PUT body as a tool, checked as the contract's `toolInputSchema` describes it.
pub fn parse_tool(body: &Value) -> Result<Tool, Refusal> {
    let body = body.as_object().ok_or_else(|| invalid("Send the tool as a JSON object."))?;
    let number = whole(body.get("number").unwrap_or(&Value::Null), NUMBER_MAX, "The tool number must be 1 to 99999.")?;
    let description =
        text(body, "description", DESCRIPTION_MAX, "The description must be text of 200 characters at most.")?;
    if description.is_empty() {
        return Err(invalid("Describe the tool, e.g. 1/4\" 2 flute end mill."));
    }
    let diameter = optional(body, "diameter")
        .map(|value| value.as_f64().filter(|d| d.is_finite() && *d > 0.0 && *d <= DIAMETER_MAX))
        .map(|diameter| diameter.ok_or_else(|| invalid("The diameter must be a number above 0 and at most 1000.")))
        .transpose()?;
    let units = optional(body, "units")
        .map(|value| serde_json::from_value::<ToolUnits>(value.clone()).map_err(|_| invalid("Units are mm or in.")))
        .transpose()?;
    if diameter.is_some() && units.is_none() {
        return Err(invalid("Give the diameter's units, mm or in."));
    }
    let flutes =
        optional(body, "flutes").map(|value| whole(value, FLUTES_MAX, "Flutes must be 1 to 20.")).transpose()?;
    let notes = text(body, "notes", NOTES_MAX, "Notes must be text of 1000 characters at most.")?;
    Ok(Tool { number, description, units: units.filter(|_| diameter.is_some()), diameter, flutes, notes })
}

fn sorted(mut tools: Vec<Tool>) -> ToolLibrary {
    tools.sort_by_key(|tool| tool.number);
    ToolLibrary { tools }
}

/// The library after adding `tool`, which must have a number of its own.
pub fn add(library: &ToolLibrary, tool: Tool) -> Result<ToolLibrary, Refusal> {
    if library.tools.iter().any(|known| known.number == tool.number) {
        return Err(Refusal::Taken(tool.number));
    }
    Ok(sorted(library.tools.iter().cloned().chain([tool]).collect()))
}

/// The library with tool `number` replaced by `tool`, which may renumber it to a number not yet taken.
pub fn replace(library: &ToolLibrary, number: u32, tool: Tool) -> Result<ToolLibrary, Refusal> {
    if !library.tools.iter().any(|known| known.number == number) {
        return Err(Refusal::Missing(number));
    }
    if tool.number != number && library.tools.iter().any(|known| known.number == tool.number) {
        return Err(Refusal::Taken(tool.number));
    }
    let others = library.tools.iter().filter(|known| known.number != number).cloned();
    Ok(sorted(others.chain([tool]).collect()))
}

pub fn remove(library: &ToolLibrary, number: u32) -> Result<ToolLibrary, Refusal> {
    let kept: Vec<Tool> = library.tools.iter().filter(|known| known.number != number).cloned().collect();
    if kept.len() == library.tools.len() {
        return Err(Refusal::Missing(number));
    }
    Ok(ToolLibrary { tools: kept })
}

/// The stored library: empty when there is none; an unreadable file is kept as `.corrupt` and read as empty.
pub fn load(paths: &Paths) -> io::Result<ToolLibrary> {
    let path = paths.tools_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(ToolLibrary::default()),
        Err(e) => return Err(e),
    };
    match serde_json::from_str::<ToolLibrary>(&text) {
        Ok(library) => Ok(sorted(library.tools)),
        Err(_) => set_aside_corrupt(&path).map(|()| ToolLibrary::default()),
    }
}

pub fn save(paths: &Paths, library: &ToolLibrary) -> io::Result<()> {
    std::fs::create_dir_all(paths.dir())?;
    let text = serde_json::to_string_pretty(library).map_err(io::Error::other)?;
    write_atomically(&paths.tools_file(), text.as_bytes())
}
