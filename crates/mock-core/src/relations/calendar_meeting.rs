use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::RecordCheckError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarMeeting {
    pub calendar_id: String,
    pub summary: String,
    pub start: String,
    pub end: String,
    pub attendees: Vec<String>,
    #[serde(default = "require_meet_default")]
    pub require_meet: bool,
}

fn require_meet_default() -> bool {
    true
}

pub fn meet_uri(source: &str) -> Option<String> {
    let parsed = url::Url::parse(source).ok()?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("meet.google.com")
        || parsed.port().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    let code = parsed.path().strip_prefix('/')?;
    let groups: Vec<_> = code.split('-').collect();
    if groups.iter().map(|group| group.len()).collect::<Vec<_>>() != [3, 4, 3]
        || !groups
            .iter()
            .all(|group| group.bytes().all(|letter| letter.is_ascii_lowercase()))
    {
        return None;
    }
    Some(format!("https://meet.google.com/{}", code))
}

pub fn meeting_uri(event: &Value) -> Option<String> {
    let mut links = Vec::new();
    for entry in event["conferenceData"]["entryPoints"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if entry["entryPointType"] == "video" {
            links.push(entry["uri"].as_str()?);
        }
    }
    if let Some(link) = event["hangoutLink"]
        .as_str()
        .filter(|link| !link.is_empty())
    {
        links.push(link);
    }
    for field in ["location", "description"] {
        if let Some(link) = event[field]
            .as_str()
            .filter(|link| meet_uri(link).is_some())
        {
            links.push(link);
        }
    }
    let distinct: BTreeSet<_> = links.into_iter().map(meet_uri).collect::<Option<_>>()?;
    (distinct.len() == 1).then(|| distinct.into_iter().next().unwrap())
}

impl CalendarMeeting {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let start = crate::clock::parse_rfc3339_nanos(&self.start);
        let end = crate::clock::parse_rfc3339_nanos(&self.end);
        let mut guests = BTreeSet::new();
        if self.calendar_id.trim().is_empty()
            || self.summary.trim().is_empty()
            || start.zip(end).is_none_or(|(start, end)| start >= end)
            || self.attendees.is_empty()
            || self.attendees.iter().any(|guest| {
                !guest.is_ascii()
                    || !guest.contains('@')
                    || guest.trim() != guest
                    || !guests.insert(guest.to_ascii_lowercase())
            })
        {
            return Err("calendar meetings require a calendar, exact title, increasing instants and distinct guest identities".into());
        }
        Ok(())
    }

    pub(super) fn owns_field(&self, field: &str) -> bool {
        [
            "/calendarId",
            "/summary",
            "/status",
            "/eventType",
            "/recurrence",
            "/start",
            "/end",
            "/attendees",
            "/conferenceData",
            "/hangoutLink",
            "/location",
        ]
        .iter()
        .any(|owned| field == *owned || field.starts_with(&format!("{owned}/")))
    }

    pub(crate) fn check(
        &self,
        record: &Value,
        state: &Value,
        complete: bool,
    ) -> Result<(), RecordCheckError> {
        let failed = |reason: &str| RecordCheckError::Failed(reason.into());
        if record["calendarId"] != self.calendar_id
            || record["summary"] != self.summary
            || record["status"] != "confirmed"
            || record
                .get("eventType")
                .is_some_and(|kind| kind != "default")
            || record.get("recurrence").is_some_and(|rules| {
                !rules.is_null() && rules.as_array().is_none_or(|rules| !rules.is_empty())
            })
        {
            return Err(failed(
                "meeting must remain one confirmed, correctly titled event on the requested calendar",
            ));
        }
        for (field, expected) in [("start", &self.start), ("end", &self.end)] {
            let actual = record[field]["dateTime"]
                .as_str()
                .and_then(crate::clock::parse_rfc3339_nanos);
            if actual != crate::clock::parse_rfc3339_nanos(expected) {
                return Err(failed(
                    "meeting boundary differs from the requested exact instant",
                ));
            }
        }
        let wanted: BTreeSet<_> = self
            .attendees
            .iter()
            .map(|guest| guest.to_ascii_lowercase())
            .collect();
        let mut present = BTreeSet::new();
        if let Some(guests) = record.get("attendees") {
            let guests = guests
                .as_array()
                .ok_or_else(|| failed("meeting guest list is not an array"))?;
            for guest in guests {
                let email = guest["email"]
                    .as_str()
                    .ok_or_else(|| failed("meeting guest lacks an identity"))?
                    .to_ascii_lowercase();
                if !wanted.contains(&email) {
                    return Err(failed("meeting includes an unrequested guest"));
                }
                present.insert(email);
            }
        }
        if complete && present != wanted {
            return Err(failed("meeting lacks the complete requested guest set"));
        }
        // A link quoted in the description's prose is not a provisioned conference: it neither
        // satisfies a required video link nor fails an event that needs none.
        let Some(uri) = meeting_uri(record) else {
            let claimed = record["hangoutLink"]
                .as_str()
                .is_some_and(|link| !link.is_empty())
                || record["conferenceData"]["entryPoints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|entry| {
                        entry["entryPointType"] == "video"
                            && entry["uri"].as_str().is_some_and(|link| !link.is_empty())
                    });
            if claimed {
                return Err(failed(
                    "meeting contains an invalid or conflicting video link",
                ));
            }
            return if complete && self.require_meet {
                Err(failed(
                    "meeting lacks one unambiguous usable Google Meet link",
                ))
            } else {
                Ok(())
            };
        };
        let known = state["services"]["google"]["calendar_conferences"].as_object();
        let issued = known
            .into_iter()
            .flat_map(|resources| resources.values())
            .any(|resource| {
                resource["solution"] == "hangoutsMeet"
                    && resource["status"] == "success"
                    && record["conferenceData"]
                        .get("conferenceId")
                        .is_none_or(|identity| resource["id"] == *identity)
                    && resource["uri"].as_str().and_then(meet_uri).as_deref() == Some(uri.as_str())
            });
        if !issued {
            return Err(failed(
                "meeting video URI lacks a successful modeled provider resource",
            ));
        }
        Ok(())
    }
}
