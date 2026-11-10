//! Ordinary wording of the free text in code and collaboration write tasks gets a pass/fail
//! verdict. Every wording travels through the mock service into state and through `grade`.

mod support;

use mock_core::grading::Grade;
use serde_json::json;
use support::{ContractTrial, prose::Proof};

async fn file_login_bug(body: String) -> Grade {
    let trial = ContractTrial::load_with_dataset(
        "code/github/file-login-bug",
        &mock_github::GitHub,
        &[],
        |_| {},
    );
    trial
        .call(
            "POST",
            "/api/v3/repos/octo-org/app/issues",
            json!({"title": "Login page returns 500", "body": body}),
        )
        .await;
    trial.assess()
}

#[tokio::test]
async fn github_file_login_bug_description() {
    Proof {
        scenario: "code/github/file-login-bug",
        field: "/body",
        also: &[],
        dictated: false,
        passes: &[
            "Started after deploy 412.",
            "Since deploy 412 went out this morning, users hitting the login page of the web app get an HTTP 500. Before that deploy the page loaded normally, so this looks like a regression introduced by it.",
            "## Summary\nThe login page returns an HTTP 500 for all users.\n\n## Details\n- **First seen:** this morning, right after deploy #412\n- **Affected page:** `/login`\n- **Impact:** users cannot sign in\n\n## Next steps\nInvestigate what changed in the 412 deployment.",
            "Hi team,\n\nUsers have been getting an HTTP 500 on the login page since this morning. The errors began once Deploy 412 was rolled out; nothing else changed in that window as far as we know.\n\nThanks!",
            "Regression: /login responds with 500 Internal Server Error. Last deployed build: 412.",
        ],
        fails: &[
            "Users hitting the login page of our web app get an HTTP 500 since this morning's deploy.",
            "Since deploy 421 went out this morning, users hitting the login page get an HTTP 500.",
            "The login page has returned HTTP 500 since deploy 4120.",
        ],
    }
    .run(file_login_bug)
    .await;
}
