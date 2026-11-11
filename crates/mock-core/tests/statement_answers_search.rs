//! Counterexamples for statement-mode answer contracts: search scenarios.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::{Value, json};
use statement_support::Case;

/// JSON pointer of `field` in the record `key` (a URL) of `collection`.
fn pointer(collection: &str, key: &str, field: &str) -> String {
    format!(
        "{collection}/{}{field}",
        key.replace('~', "~0").replace('/', "~1")
    )
}

/// Neither verdict earns reward; which of the two the reader returns is incidental.
#[track_caller]
fn no_reward(case: &Case, answers: &[&str]) {
    for answer in answers {
        let outcome = case.outcome(answer);
        assert!(
            matches!(outcome, Outcome::Fail | Outcome::Unresolved),
            "answer: {answer}\noutcome: {outcome:?}"
        );
    }
}

/// The authored text at `pointer` with `old` replaced by `new`.
fn rewritten(case: &Case, pointer: &str, old: &str, new: &str) -> Value {
    let text = case
        .initial
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no authored text at {pointer}"));
    assert!(text.contains(old), "{pointer} does not contain {old:?}");
    json!(text.replace(old, new))
}

#[test]
fn brave_funding_round_is_read_from_this_weeks_headline() {
    let case = Case::load("search/brave/competitor-funding-news");
    case.reference_is_source_bound();
    case.passes(&[
        "Amount: $45M\nRound: Series B\nLead investor: Halcyon Ventures",
        "Norvik Analytics raised USD 45 million in a Series B round led by Halcyon Ventures.",
        "45 million dollars, series B, led by Halcyon Ventures",
        "$45mn Series B, lead: Halcyon Ventures; existing investor Fjord Capital also participated.",
        // The total raised to date is a different quantity, not a competing round.
        "$45M Series B led by Halcyon Ventures, bringing total funding to $61 million.",
        // A polite hedge still states the answer, and an amount may be spelled out.
        "It was probably a $45M Series B led by Halcyon Ventures.",
        "forty-five million dollars, Series B, led by Halcyon Ventures.",
        // An amount the source gives in millions, written out in full.
        "$45,000,000 Series B led by Halcyon Ventures.",
        // Last year's round, named after the answer and as what it is, is context.
        "$45M Series B led by Halcyon Ventures. Their 2025 Series A was $12M led by Fjord Capital.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "They did not raise $45M; it was not a Series B led by Halcyon Ventures.",
        "Rather than a $45M Series B led by Halcyon Ventures, it was a $12M Series A led by Fjord Capital.",
        // Last year's round and the look-alike company's round.
        "Norvik Analytics closed a $12M Series A led by Fjord Capital.",
        "Norvik raised $30M led by Brightline Partners.",
        // The right round with the participating investor as its lead, the
        // earlier round's name, and the total raised to date as the amount.
        "$45M Series B led by Fjord Capital.",
        "$45M Series A led by Halcyon Ventures.",
        "$61 million Series B led by Halcyon Ventures.",
        // Near-miss amounts and magnitudes.
        "$54M Series B led by Halcyon Ventures.",
        "$45.5M Series B led by Halcyon Ventures.",
        "$45K Series B led by Halcyon Ventures.",
        "$45 billion Series B led by Halcyon Ventures.",
        // The right magnitude in another currency.
        "€45 million Series B led by Halcyon Ventures.",
    ]);
    case.unresolved(&[
        // The magnitude is part of the amount: a bare 45 is not read.
        "$45 Series B led by Halcyon Ventures.",
        // Half of the lead investor's name is not its name.
        "$45M Series B led by Halcyon.",
    ]);
    // The facts follow the authored headline.
    let title = pointer(
        "/services/brave/news",
        "https://www.techbeat.io/2026/10/27/norvik-analytics-series-b",
        "/title",
    );
    let edited = json!("Norvik Analytics raises $52M Series C led by Halcyon Ventures");
    assert_eq!(
        case.outcome_after(&title, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(&title, edited, "$52M Series C led by Halcyon Ventures."),
        Outcome::Pass
    );
}

#[test]
fn brave_sla_commitment_is_read_from_the_sla_in_force() {
    let case = Case::load("search/brave/hosting-sla-lookup");
    case.reference_is_source_bound();
    case.passes(&[
        "Uptime: 99.95%\nService credit: 10% of the monthly fee",
        "The Business plan commits to 99.95 percent monthly uptime; the credit is 10 percent of the monthly fee.",
        "| Uptime | 99.95 % |\n| Credit | 10 % |",
        "99.95% (credit: 10%; 25% if uptime drops below 99.0%)",
        // A polite hedge still states the answer.
        "I believe it is 99.95% with a 10% credit.",
        // The archived values, named after the answer and as archived, are context.
        "99.95% uptime, 10% credit. The archived 2024 SLA said 99.9% and 5%.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "It is not 99.95% and the credit is not 10%.",
        "Rather than 99.95% with a 10% credit, the SLA commits to 99.9% with a 5% credit.",
        // The archived SLA, the look-alike provider and the deeper credit tier.
        "99.9% monthly uptime with a 5% service credit.",
        "99.99% uptime with a 20% service credit.",
        "99.95% monthly uptime with a 25% service credit.",
        // One current value paired with one archived value, and a near miss.
        "99.9% monthly uptime with a 10% service credit.",
        "99.95% monthly uptime with a 5% service credit.",
        "99.5% monthly uptime with a 10% service credit.",
        // The two percentages in each other's role.
        "10% monthly uptime with a service credit of 99.95%.",
        "Uptime: 10%\nCredit: 99.95%",
    ]);
    case.unresolved(&[
        // Percentages without their unit are not read.
        "99.95 uptime and a credit of 10.",
    ]);
    // A spelled-out decimal is not read. The reader takes its last word for another
    // percentage ("five percent") and fails the reply instead of asking for adjudication.
    no_reward(
        &case,
        &["Uptime is ninety-nine point nine five percent; the credit is ten percent."],
    );
    // The facts follow the authored plan table.
    let content = pointer(
        "/services/brave/web",
        "https://www.ironbarkhosting.com/legal/sla",
        "/content",
    );
    let edited = rewritten(
        &case,
        &content,
        "| Business | 99.95% | 10% of the monthly fee below 99.95%",
        "| Business | 99.97% | 15% of the monthly fee below 99.97%",
    );
    assert_eq!(
        case.outcome_after(&content, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            &content,
            edited,
            "99.97% monthly uptime, 15% service credit."
        ),
        Outcome::Pass
    );
}

#[test]
fn brave_keynote_recording_is_the_conference_channels_full_video() {
    let case = Case::load("search/brave/keynote-recording");
    case.reference_is_source_bound();
    case.passes(&[
        "Marta Lindqvist gave the keynote; the video runs 47:12: https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        "- Speaker: Marta Lindqvist\n- Length: 47:12 (mm:ss)\n- Link: [PGConf Nordic](https://www.youtube.com/watch?v=Zt4mQeL1x9c)",
        // A polite hedge still states the answer, and a spoken duration is read.
        "It seems to be Marta Lindqvist, 47:12, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        "Marta Lindqvist, 47 minutes 12 seconds, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "The speaker is not Marta Lindqvist; 47:12, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        "Rather than https://www.youtube.com/watch?v=Zt4mQeL1x9c (Marta Lindqvist, 47:12), watch https://www.youtube.com/watch?v=r8Kq2PgN7aE.",
        // The highlights cut, the earlier meetup version and another speaker's talk.
        "Marta Lindqvist, 12:05, https://www.youtube.com/watch?v=r8Kq2PgN7aE",
        "Marta Lindqvist, 28:40, https://www.youtube.com/watch?v=Hq7wB3nVt2s",
        "Tom Ekberg, 39:55, https://vimeo.com/987654321",
        // The right video with another talk's speaker or another video's length.
        "Tom Ekberg, 47:12, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        "Marta Lindqvist, 12:05, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        // Near-miss duration.
        "Marta Lindqvist, 47:21, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        // A denial across a label noun or a Markdown link label.
        "The link is not the video https://www.youtube.com/watch?v=Zt4mQeL1x9c. Marta Lindqvist, 47:12.",
        "It is not [this video](https://www.youtube.com/watch?v=Zt4mQeL1x9c). Marta Lindqvist, 47:12.",
    ]);
    case.unresolved(&[
        // A negated verb after the speaker disputes her.
        "Marta Lindqvist did not give this keynote. 47:12, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        // A surname alone and a short link are not the authored values.
        "Lindqvist, 47:12, https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        "Marta Lindqvist, 47:12, https://youtu.be/Zt4mQeL1x9c",
    ]);
    // The facts follow the authored video record.
    let duration = pointer(
        "/services/brave/videos",
        "https://www.youtube.com/watch?v=Zt4mQeL1x9c",
        "/duration",
    );
    assert_eq!(
        case.outcome_after(&duration, json!("51:03"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            &duration,
            json!("51:03"),
            "Marta Lindqvist, 51:03, https://www.youtube.com/watch?v=Zt4mQeL1x9c"
        ),
        Outcome::Pass
    );
}

#[test]
fn exa_timeout_option_is_read_from_the_v4_reference() {
    let case = Case::load("search/exa/sdk-timeout-default");
    case.reference_is_source_bound();
    case.passes(&[
        "Option: `timeout_seconds`\nDefault: 30 seconds",
        "Use timeout_seconds; the default is 30s.",
        "driftwood.Client(timeout_seconds=30) is the default.",
        "timeout_seconds, default 30 (seconds)",
        // The same duration in another unit.
        "timeout_seconds, default 30000 ms",
        // Reviewed remarks about the v3 option that the v4 page itself makes.
        "timeout_seconds (replaces v3's request_timeout), default 30 seconds.",
        "timeout_seconds instead of request_timeout; default 30 seconds.",
        // A polite hedge still states the answer, and a number may be spelled out.
        "I think it is timeout_seconds with a default of 30 seconds.",
        "timeout_seconds, default thirty seconds",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "The option is not timeout_seconds and the default is not 30 seconds.",
        "Rather than timeout_seconds (30 seconds), use request_timeout, which defaults to 60 seconds.",
        // The legacy v3 page and the Node.js SDK.
        "request_timeout, default 60 seconds",
        "timeoutMs, default 120000 ms",
        // One v4 value paired with one v3 value, and the README's example value.
        "request_timeout, default 30 seconds",
        "timeout_seconds, default 60 seconds",
        "timeout_seconds, default 10 seconds",
        // The right number in the wrong unit.
        "timeout_seconds, default 30 minutes",
        "timeout_seconds, default 30 ms",
    ]);
    case.unresolved(&[
        // A bare number carries no unit, and a shortened option name is not the option.
        "timeout_seconds, default 30",
        "timeout, default 30 seconds",
    ]);
    // The facts follow the authored documentation text.
    let text = pointer(
        "/services/exa/documents",
        "https://docs.driftwood.dev/python/v4/configuration",
        "/text",
    );
    let edited = rewritten(
        &case,
        &text,
        "defaults to 30 seconds",
        "defaults to 45 seconds",
    );
    assert_eq!(
        case.outcome_after(&text, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(&text, edited, "timeout_seconds, default 45 seconds"),
        Outcome::Pass
    );
}

#[test]
fn exa_profile_is_selected_by_the_current_role_not_by_its_url() {
    let case = Case::load("search/exa/find-vp-engineering-profile");
    case.reference_is_source_bound();
    case.passes(&[
        "https://www.linkedin.com/in/priya-raman-tessellate",
        "Profile: [Priya Raman](https://www.linkedin.com/in/priya-raman-tessellate/) (role confirmed on https://www.tessellate.bio/team)",
        "https://www.linkedin.com/in/priya-raman-tessellate, not https://www.linkedin.com/in/priya-raman-4b21a9.",
        // A polite hedge still states the answer, and a link may omit its scheme.
        "It is probably https://www.linkedin.com/in/priya-raman-tessellate.",
        "www.linkedin.com/in/priya-raman-tessellate",
        "linkedin.com/in/priya-raman-tessellate",
        // The former director, named after the answer and as former, is context.
        "https://www.linkedin.com/in/priya-raman-tessellate (the former director is https://www.linkedin.com/in/priya-raman-4b21a9).",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keyword.
        "It is not https://www.linkedin.com/in/priya-raman-tessellate.",
        "It is not the profile https://www.linkedin.com/in/priya-raman-tessellate.",
        "It is not [her profile](https://www.linkedin.com/in/priya-raman-tessellate).",
        "Rather than https://www.linkedin.com/in/priya-raman-tessellate, use https://www.linkedin.com/in/priya-raman-4b21a9.",
        // The former director, the namesake at a similarly named company, the
        // cardiologist, and the company page that is not a profile.
        "https://www.linkedin.com/in/priya-raman-4b21a9",
        "https://www.linkedin.com/in/priyaraman",
        "https://www.linkedin.com/in/dr-priya-raman-md",
        "https://www.tessellate.bio/team",
    ]);
    case.unresolved(&[
        // Altered forms of the URL are not the URL.
        "Her LinkedIn handle is priya-raman-tessellate.",
        "https://www.linkedin.com/in/priya-raman-tessellate-bio",
    ]);
    // The profile follows the authored roles: once the former director's
    // record holds the role instead, her URL is the answer.
    let documents = "/services/exa/documents";
    let current = "https://www.linkedin.com/in/priya-raman-tessellate";
    let former = "https://www.linkedin.com/in/priya-raman-4b21a9";
    let mut edited = case.initial.clone();
    for (key, field, value) in [
        (
            current,
            "/title",
            "Priya Raman - Advisor - Tessellate Bio | LinkedIn",
        ),
        (
            former,
            "/title",
            "Priya Raman - VP of Engineering - Tessellate Bio | LinkedIn",
        ),
        (
            former,
            "/text",
            "Priya Raman. Experience: Tessellate Bio, VP of Engineering (2026-present).",
        ),
    ] {
        *edited.pointer_mut(&pointer(documents, key, field)).unwrap() = json!(value);
    }
    assert_eq!(
        case.grade_with(&edited, &case.reference).outcome,
        Outcome::Fail
    );
    assert_eq!(case.grade_with(&edited, former).outcome, Outcome::Pass);
    // A changed URL on the selected record changes the fact as well.
    let url = pointer(documents, current, "/url");
    let moved = "https://www.linkedin.com/in/priya-raman-vpe";
    assert_eq!(case.outcome_after(&url, json!(moved), moved), Outcome::Pass);
    assert_eq!(
        case.outcome_after(&url, json!(moved), &case.reference),
        Outcome::Unresolved
    );
}

#[test]
fn firecrawl_team_plan_is_read_from_the_live_pricing_page() {
    let case = Case::load("search/firecrawl/northwind-team-pricing");
    case.reference_is_source_bound();
    case.passes(&[
        "Team plan: $79/month billed annually, 12 seats included.",
        "Price: USD 79 per month (annual billing)\nSeats: 12",
        "| Price | 79 USD |\n| Seats included | 12 seats |",
        "$79.00/month, 12 users",
        // The monthly-billing price and the add-on seat price are other quantities.
        "$79 per month billed annually ($99 billed monthly), 12 seats, extra seats $8/seat/month.",
        // A polite hedge still states the answer, and numbers may be spelled out.
        "Probably $79/month with 12 seats.",
        "seventy-nine dollars per month with twelve seats.",
        // The stale values, named after the answer and as the 2025 post's, are context.
        "Team: $79/month billed annually, 12 seats (the 2025 blog post said $69 and 10 seats).",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "It is not $79 per month and it does not include 12 seats.",
        "Rather than $79 with 12 seats, the Team plan is $69 with 10 seats.",
        // The 2025 blog post and the third-party listing, the monthly-billing
        // price, and another plan.
        "$69/month billed annually with 10 seats.",
        "$99/month with 12 seats.",
        "$249/month with 40 seats.",
        // One current value paired with one stale value.
        "$79/month with 10 seats.",
        "$69/month with 12 seats.",
        // Near misses, another currency and the yearly total.
        "$97/month with 12 seats.",
        "$79/month with 21 seats.",
        "€79/month with 12 seats.",
        "$948 per year with 12 seats.",
    ]);
    case.unresolved(&[
        // A number without its currency or its unit is not read.
        "79 per month with 12 seats.",
        "$79 per month with 12 included.",
    ]);
    // The facts follow the authored pricing page.
    let markdown = pointer(
        "/services/firecrawl/pages",
        "https://www.northwind-analytics.com/pricing",
        "/markdown",
    );
    let edited = rewritten(
        &case,
        &markdown,
        "- 12 seats included",
        "- 15 seats included",
    );
    assert_eq!(
        case.outcome_after(&markdown, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(&markdown, edited, "$79/month billed annually, 15 seats."),
        Outcome::Pass
    );
}

#[test]
fn tavily_rate_limit_is_read_from_the_current_reference_and_cited() {
    let case = Case::load("search/tavily/vendor-rate-limit");
    case.reference_is_source_bound();
    case.passes(&[
        "1200 requests/min for Business, from https://docs.quillstream.io/api/rate-limits",
        "Business plan: 1,200 rpm. Found at [Rate limits](https://docs.quillstream.io/api/rate-limits).",
        "1,200 req/min (https://docs.quillstream.io/api/rate-limits)",
        "1,200/min (https://docs.quillstream.io/api/rate-limits)",
        "1,200 per minute (https://docs.quillstream.io/api/rate-limits)",
        "1200 requests a minute (https://docs.quillstream.io/api/rate-limits)",
        // A scale suffix before the unit.
        "1.2k requests per minute (https://docs.quillstream.io/api/rate-limits)",
        // A reviewed aside about the deprecated figure.
        "1,200 requests per minute (https://docs.quillstream.io/api/rate-limits); the deprecated v2 docs said 600 requests per minute.",
        "1,200 requests per minute (https://docs.quillstream.io/api/rate-limits). The v2 page https://docs.quillstream.io/v2/api/rate-limits is deprecated.",
        // A polite hedge still states the answer, and a link may omit its scheme.
        "I believe it is 1,200 requests per minute, from https://docs.quillstream.io/api/rate-limits",
        "1,200 requests per minute, per docs.quillstream.io/api/rate-limits",
        // Older figures, named after the answer and as older, are context.
        "1,200 requests per minute (https://docs.quillstream.io/api/rate-limits); older posts mention 600/min and 900 per minute.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "It is not 1,200 requests per minute (https://docs.quillstream.io/api/rate-limits).",
        "The limit is not 1,200/min (https://docs.quillstream.io/api/rate-limits).",
        "600/min (https://docs.quillstream.io/api/rate-limits)",
        "Rather than 1,200 requests per minute (https://docs.quillstream.io/api/rate-limits), the limit is 600 requests per minute.",
        // The deprecated v2 page, the forum post, the look-alike vendor and another plan.
        "600 requests per minute, from https://docs.quillstream.io/v2/api/rate-limits",
        "900 requests per minute, from https://community.quillstream.io/t/business-plan-rate-limit/4412",
        "300 requests per minute, from https://docs.quillpad.com/api/limits",
        "400 requests per minute, from https://docs.quillstream.io/api/rate-limits",
        // The deprecated number on the current page, and the current number
        // attributed to the deprecated page or to no page at all.
        "600 requests per minute, from https://docs.quillstream.io/api/rate-limits",
        "1,200 requests per minute, from https://docs.quillstream.io/v2/api/rate-limits",
        "1,200 requests per minute.",
        // Near-miss numbers.
        "1,020 requests per minute, from https://docs.quillstream.io/api/rate-limits",
        "12,000 requests per minute, from https://docs.quillstream.io/api/rate-limits",
    ]);
    case.unresolved(&[
        // A spelled-out number above ninety-nine is not read.
        "twelve hundred requests per minute, from https://docs.quillstream.io/api/rate-limits",
        // A number without its unit, and a source described but not cited.
        "1,200 (https://docs.quillstream.io/api/rate-limits)",
        "1,200 per hour (https://docs.quillstream.io/api/rate-limits)",
        "1,200 requests per minute, per the Quillstream API v3 rate limits page.",
    ]);
    // The facts follow the authored rate-limit table.
    let raw = pointer(
        "/services/tavily/pages",
        "https://docs.quillstream.io/api/rate-limits",
        "/raw_content",
    );
    let edited = rewritten(
        &case,
        &raw,
        "| Business | 1,200 | 200 |",
        "| Business | 1,500 | 200 |",
    );
    assert_eq!(
        case.outcome_after(&raw, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            &raw,
            edited,
            "1,500 requests per minute, from https://docs.quillstream.io/api/rate-limits"
        ),
        Outcome::Pass
    );
}

/// A notation the reader does not accept. It is safe (a correct answer in it is never
/// scored as a pass for something wrong), but stricter than the keyword predicates
/// these contracts replace.
#[test]
fn notations_the_reader_does_not_accept_never_pass() {
    // Digits are not read inside a hyphenated compound.
    Case::load("search/firecrawl/northwind-team-pricing")
        .fails(&["$79 per month for a 12-seat team."]);
}

#[test]
fn tavily_signature_sunset_is_the_date_of_the_signature_guide() {
    let case = Case::load("search/tavily/webhook-signature-sunset");
    case.reference_is_source_bound();
    case.passes(&[
        "Date: 2027-01-15\nHeader: Ferrowave-Signature-V2",
        "v1 signatures stop on 15 January 2027; verify the `Ferrowave-Signature-V2` header instead.",
        "Jan 15, 2027 - Ferrowave-Signature-V2",
        "Ferrowave will no longer send v1 signatures after January 15, 2027; switch to Ferrowave-Signature-V2.",
        // Denying the look-alike deadline and the old header leaves the answer intact.
        "January 15, 2027, not December 1, 2026; verify Ferrowave-Signature-V2, not X-Ferrowave-Sig.",
        // A polite hedge still states the answer.
        "I believe it is January 15, 2027; verify Ferrowave-Signature-V2.",
        // The API-key deadline, named after the answer and as what it is, is context.
        "January 15, 2027; verify Ferrowave-Signature-V2. (v1 API keys stop working on December 1, 2026.)",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "It is not January 15, 2027, and the header is not Ferrowave-Signature-V2.",
        "Rather than January 15, 2027, the sunset is December 1, 2026; verify Ferrowave-Signature-V2.",
        // The look-alike deadlines: v1 API keys, the 3.x SDK and the archived v1 page.
        "December 1, 2026; verify Ferrowave-Signature-V2.",
        "2026-11-30; verify Ferrowave-Signature-V2.",
        "June 30, 2026; verify Ferrowave-Signature-V2.",
        // The right date with the header that is being retired, or a truncated one.
        "January 15, 2027; keep verifying X-Ferrowave-Sig.",
        "January 15, 2027; verify Ferrowave-Signature.",
        // Near-miss dates and a date without a day.
        "January 16, 2027; verify Ferrowave-Signature-V2.",
        "January 15, 2026; verify Ferrowave-Signature-V2.",
        "Sometime in 2027; verify Ferrowave-Signature-V2.",
    ]);
    case.unresolved(&[
        // Date notations the reader does not accept.
        "01/15/2027; verify Ferrowave-Signature-V2.",
        "the fifteenth of January 2027; verify Ferrowave-Signature-V2.",
    ]);
    // The date follows the authored guide.
    let raw = pointer(
        "/services/tavily/pages",
        "https://docs.ferrowave.dev/guides/webhooks/verify-signatures",
        "/raw_content",
    );
    let edited = rewritten(&case, &raw, "January 15, 2027", "March 2, 2027");
    assert_eq!(
        case.outcome_after(&raw, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(&raw, edited, "2027-03-02; verify Ferrowave-Signature-V2."),
        Outcome::Pass
    );
}

#[test]
fn multi_funding_round_and_growth_price_come_from_two_independent_sources() {
    let case = Case::load("search/multi/competitor-funding-and-pricing");
    case.reference_is_source_bound();
    case.passes(&[
        "Funding: $45M Series B led by Halcyon Ventures.\nPricing: Growth is $890 per month, billed annually.",
        "| Raised | USD 45 million |\n| Round | Series B |\n| Lead | Halcyon Ventures |\n| Growth (annual billing) | 890 USD/month |",
        "$45M Series B led by Halcyon Ventures. Growth is 890 per month.",
        // The monthly-billing price is another quantity of the same page.
        "$45M Series B led by Halcyon Ventures. Growth is $890/month billed annually, or $1,090 month to month.",
        // A polite hedge still states the answer, and an amount may be spelled out.
        "$45M Series B led by Halcyon Ventures. Growth is probably $890/month.",
        "forty-five million dollars, Series B led by Halcyon Ventures. Growth is $890/month.",
        // The stale price, named after the answer and as the 2025 post's, is context.
        "$45M Series B led by Halcyon Ventures. Growth is $890/month billed annually (the 2025 blog said $690).",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "$45M Series B led by Halcyon Ventures. Growth is not $890/month.",
        "$45M Series B led by Halcyon Ventures. Rather than $890/month, Growth costs $690/month.",
        "It was not a Series B led by Halcyon Ventures, though they raised $45M; Growth is $890/month.",
        // Last year's round and the look-alike company's round, with the right price.
        "$12M Series A led by Fjord Capital. Growth is $890/month.",
        "$30M led by Brightline Partners. Growth is $890/month.",
        // The right round with the monthly-billing price, the stale blog and
        // listing price, another plan's price and the look-alike's price.
        "$45M Series B led by Halcyon Ventures. Growth is $1,090/month.",
        "$45M Series B led by Halcyon Ventures. Growth is $690/month.",
        "$45M Series B led by Halcyon Ventures. Growth is $290/month.",
        "$45M Series B led by Halcyon Ventures. Growth is $1,650 per robot per month.",
        // Near misses, another currency and the yearly total.
        "$45M Series B led by Halcyon Ventures. Growth is $980/month.",
        "$54M Series B led by Halcyon Ventures. Growth is $890/month.",
        "$45M Series B led by Halcyon Ventures. Growth is €890/month.",
        "$45M Series B led by Halcyon Ventures. Growth is $10,680 per year.",
        // Only one of the two questions answered.
        "$45M Series B led by Halcyon Ventures.",
        "Growth is $890/month.",
    ]);
    case.unresolved(&[
        // The source number without a price label is not read as the price.
        "$45M Series B led by Halcyon Ventures; they have 890 customers.",
        "They did not raise $45M in a Series B led by Halcyon Ventures; Growth is $890/month.",
        "$45M Series B led by Halcyon. Growth is $890/month.",
    ]);
    // Each fact follows its own source.
    let markdown = pointer(
        "/services/firecrawl/pages",
        "https://www.norvikanalytics.com/pricing",
        "/markdown",
    );
    let repriced = rewritten(&case, &markdown, "$890/month", "$920/month");
    assert_eq!(
        case.outcome_after(&markdown, repriced.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            &markdown,
            repriced,
            "$45M Series B led by Halcyon Ventures. Growth is $920/month."
        ),
        Outcome::Pass
    );
    let title = pointer(
        "/services/brave/news",
        "https://www.techbeat.io/2026/10/27/norvik-analytics-series-b",
        "/title",
    );
    let retitled = json!("Norvik Analytics raises $52M Series C led by Halcyon Ventures");
    assert_eq!(
        case.outcome_after(&title, retitled.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            &title,
            retitled,
            "$52M Series C led by Halcyon Ventures. Growth is $890/month."
        ),
        Outcome::Pass
    );
    // An amount in the hundreds is not read when spelled out. The reader takes its last
    // word for another amount ("ninety dollars") and fails the reply instead of asking
    // for adjudication.
    no_reward(
        &case,
        &["$45M Series B led by Halcyon Ventures. Growth is eight hundred ninety dollars a month."],
    );
}
