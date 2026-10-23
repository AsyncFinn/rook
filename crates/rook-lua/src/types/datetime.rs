//! DateTime: a moment as milliseconds since the Unix epoch, with its
//! calendar parts in UTC or the machine's time zone (creator-docs
//! DateTime).

use chrono::{DateTime, Datelike, Local, LocalResult, NaiveDate, TimeZone, Timelike, Utc};
use mlua::{Lua, MetaMethod, UserData, UserDataFields, UserDataMethods, UserDataRef};

use super::err;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LDateTime {
    millis: i64,
}

impl LDateTime {
    fn utc(self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.millis)
            .single()
            .unwrap_or_default()
    }

    fn local(self) -> DateTime<Local> {
        self.utc().with_timezone(&Local)
    }
}

/// `{Year, Month, Day, Hour, Minute, Second, Millisecond}`.
fn parts<T: TimeZone>(lua: &Lua, t: &DateTime<T>) -> mlua::Result<mlua::Table> {
    let p = lua.create_table()?;
    p.set("Year", t.year())?;
    p.set("Month", t.month())?;
    p.set("Day", t.day())?;
    p.set("Hour", t.hour())?;
    p.set("Minute", t.minute())?;
    p.set("Second", t.second())?;
    p.set("Millisecond", t.timestamp_subsec_millis())?;
    Ok(p)
}

/// Roblox's `FormatUniversalTime` tokens (the common ones of moment.js
/// that creator-docs lists), in `en-us`; other text is kept.
fn format<T: TimeZone>(t: &DateTime<T>, pattern: &str) -> String
where
    T::Offset: std::fmt::Display,
{
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    const DAYS: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let month = MONTHS[t.month0() as usize];
    let day = DAYS[t.weekday().num_days_from_monday() as usize];
    let hour12 = match t.hour() % 12 {
        0 => 12,
        h => h,
    };
    // Longest tokens first, so "MMMM" isn't read as "MM" twice.
    let tokens: [(&str, String); 28] = [
        ("YYYY", format!("{:04}", t.year())),
        ("MMMM", month.to_owned()),
        ("dddd", day.to_owned()),
        (
            "LLLL",
            format!(
                "{day}, {month} {}, {} {hour12}:{:02} {}",
                t.day(),
                t.year(),
                t.minute(),
                am_pm(t.hour())
            ),
        ),
        (
            "LLL",
            format!(
                "{month} {}, {} {hour12}:{:02} {}",
                t.day(),
                t.year(),
                t.minute(),
                am_pm(t.hour())
            ),
        ),
        ("MMM", month[..3].to_owned()),
        ("ddd", day[..3].to_owned()),
        (
            "LTS",
            format!(
                "{hour12}:{:02}:{:02} {}",
                t.minute(),
                t.second(),
                am_pm(t.hour())
            ),
        ),
        ("YY", format!("{:02}", t.year() % 100)),
        ("MM", format!("{:02}", t.month())),
        ("DD", format!("{:02}", t.day())),
        ("HH", format!("{:02}", t.hour())),
        ("hh", format!("{hour12:02}")),
        ("mm", format!("{:02}", t.minute())),
        ("ss", format!("{:02}", t.second())),
        ("LL", format!("{month} {}, {}", t.day(), t.year())),
        (
            "LT",
            format!("{hour12}:{:02} {}", t.minute(), am_pm(t.hour())),
        ),
        ("Do", ordinal(t.day())),
        ("dd", day[..2].to_owned()),
        ("SSS", format!("{:03}", t.timestamp_subsec_millis())),
        ("M", t.month().to_string()),
        ("D", t.day().to_string()),
        ("H", t.hour().to_string()),
        ("h", hour12.to_string()),
        ("m", t.minute().to_string()),
        ("s", t.second().to_string()),
        ("A", am_pm(t.hour()).to_owned()),
        ("a", am_pm(t.hour()).to_lowercase()),
    ];
    let mut out = String::new();
    let mut rest = pattern;
    'outer: while !rest.is_empty() {
        // Text in brackets is kept as written.
        if let Some(stripped) = rest.strip_prefix('[')
            && let Some(end) = stripped.find(']')
        {
            out.push_str(&stripped[..end]);
            rest = &stripped[end + 1..];
            continue;
        }
        for (token, value) in &tokens {
            if let Some(after) = rest.strip_prefix(token) {
                out.push_str(value);
                rest = after;
                continue 'outer;
            }
        }
        let c = rest.chars().next().expect("not empty");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn am_pm(hour: u32) -> &'static str {
    if hour < 12 { "AM" } else { "PM" }
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

impl UserData for LDateTime {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "DateTime");
        f.add_field_method_get("UnixTimestamp", |_, t| Ok(t.millis.div_euclid(1000)));
        f.add_field_method_get("UnixTimestampMillis", |_, t| Ok(t.millis));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("ToUniversalTime", |lua, t, ()| parts(lua, &t.utc()));
        m.add_method("ToLocalTime", |lua, t, ()| parts(lua, &t.local()));
        m.add_method("ToIsoDate", |_, t, ()| {
            Ok(t.utc().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        });
        m.add_method(
            "FormatUniversalTime",
            |_, t, (pattern, _locale): (String, Option<String>)| Ok(format(&t.utc(), &pattern)),
        );
        m.add_method(
            "FormatLocalTime",
            |_, t, (pattern, _locale): (String, Option<String>)| Ok(format(&t.local(), &pattern)),
        );
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<Self>| Ok(*t == *o));
        m.add_meta_method(MetaMethod::Lt, |_, t, o: UserDataRef<Self>| Ok(*t < *o));
        m.add_meta_method(MetaMethod::Le, |_, t, o: UserDataRef<Self>| Ok(*t <= *o));
        // Inference: the Unix milliseconds, from Roblox's bubble chat
        // ordering bubbles by `tonumber(tostring(timestamp):sub(-9))`.
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| Ok(t.millis.to_string()));
    }
}

type Parts = (
    i32,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
    Option<u32>,
);

/// Calendar parts to a moment; out-of-range parts carry over, as Roblox's
/// do (creator-docs DateTime.fromUniversalTime).
fn from_parts<T: TimeZone>(zone: &T, p: Parts) -> mlua::Result<LDateTime> {
    let (year, month, day, hour, minute, second, millis) = p;
    let date = NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(|| err("DateTime: invalid year"))?;
    let months = i64::from(month.unwrap_or(1)) - 1;
    let total_months = i64::from(year) * 12 + months;
    let date = NaiveDate::from_ymd_opt(
        i32::try_from(total_months.div_euclid(12)).map_err(|_| err("DateTime: invalid month"))?,
        u32::try_from(total_months.rem_euclid(12) + 1).unwrap_or(1),
        1,
    )
    .unwrap_or(date);
    let naive = date.and_hms_opt(0, 0, 0).expect("midnight")
        + chrono::Duration::days(i64::from(day.unwrap_or(1)) - 1)
        + chrono::Duration::hours(i64::from(hour.unwrap_or(0)))
        + chrono::Duration::minutes(i64::from(minute.unwrap_or(0)))
        + chrono::Duration::seconds(i64::from(second.unwrap_or(0)))
        + chrono::Duration::milliseconds(i64::from(millis.unwrap_or(0)));
    let moment = match zone.from_local_datetime(&naive) {
        LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => t,
        LocalResult::None => return Err(err("DateTime: that local time doesn't exist")),
    };
    Ok(LDateTime {
        millis: moment.timestamp_millis(),
    })
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let t = lua.create_table()?;
    t.set(
        "now",
        lua.create_function(|_, ()| {
            Ok(LDateTime {
                millis: Utc::now().timestamp_millis(),
            })
        })?,
    )?;
    t.set(
        "fromUnixTimestamp",
        lua.create_function(|_, s: f64| {
            Ok(LDateTime {
                millis: (s * 1000.0) as i64,
            })
        })?,
    )?;
    t.set(
        "fromUnixTimestampMillis",
        lua.create_function(|_, ms: f64| Ok(LDateTime { millis: ms as i64 }))?,
    )?;
    t.set(
        "fromUniversalTime",
        lua.create_function(|_, p: Parts| from_parts(&Utc, p))?,
    )?;
    t.set(
        "fromLocalTime",
        lua.create_function(|_, p: Parts| from_parts(&Local, p))?,
    )?;
    t.set(
        "fromIsoDate",
        lua.create_function(|_, s: String| {
            Ok(DateTime::parse_from_rfc3339(&s).ok().map(|t| LDateTime {
                millis: t.timestamp_millis(),
            }))
        })?,
    )?;
    lua.globals().set("DateTime", t)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_carry_over_and_format() {
        // 2020-02-30 is 2020-03-01.
        let t = from_parts(
            &Utc,
            (2020, Some(2), Some(30), Some(13), Some(5), Some(0), None),
        )
        .unwrap();
        assert_eq!(
            format(&t.utc(), "YYYY-MM-DD HH:mm [at] h A, MMMM Do"),
            "2020-03-01 13:05 at 1 PM, March 1st"
        );
    }
}
