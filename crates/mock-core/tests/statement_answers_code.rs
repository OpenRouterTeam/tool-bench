//! Counterexamples for statement-mode answer contracts: code scenarios.

mod statement_support;

use mock_core::grading::Outcome;
use serde_json::json;
use statement_support::Case;

#[test]
fn argocd_degraded_app_binds_the_app_to_its_pods_image_tag_and_reason() {
    let case = Case::load("code/argocd/find-degraded-app");
    case.reference_is_source_bound();
    case.passes(&[
        "- Application: checkout-web\n- Failing workload: Deployment checkout-web (pod checkout-web-7f8b9c6d5-q2x7m)\n- Root cause: ErrImagePull, the image tag 2.14.0-rc3 was never pushed",
        "checkout-web is Degraded: its new pod cannot pull ghcr.io/lumen-retail/checkout-web:2.14.0-rc3 (ImagePullBackOff).",
        "| App | checkout-web |\n| Tag | 2.14.0-rc3 |\n| Why | the kubelet failed to pull the image |",
        // A polite hedge still states the answer.
        "I think the Degraded app is probably checkout-web, failing to pull tag 2.14.0-rc3 (ImagePullBackOff).",
        // Other authored records named beside or after the answer are context.
        "checkout-web (2.14.0-rc3, ImagePullBackOff) is Degraded; checkout-web-staging is Degraded too.",
        "checkout-web is Degraded: the 2.14.0-rc3 pod is in ImagePullBackOff while the 2.13.4 pods keep serving.",
        // The root cause the pod's events give, without the Kubernetes status word: the
        // registry does not have the tag.
        "checkout-web is Degraded on production because the image tag 2.14.0-rc3 does not exist in the registry.",
        "Application: checkout-web. Failing resource: Deployment checkout-web. Root cause: image tag 2.14.0-rc3 was not found in ghcr.io (NotFound).",
        "The Degraded production app is checkout-web: its Deployment was pointed at ghcr.io/lumen-retail/checkout-web:2.14.0-rc3, a tag that was never pushed.",
        "App: checkout-web. Root cause: missing image tag - ghcr.io/lumen-retail/checkout-web:2.14.0-rc3 is not in the registry.",
        "checkout-web: the Deployment checkout-web cannot roll out because the registry has no 2.14.0-rc3 tag.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "The Degraded production app is not checkout-web; tag 2.14.0-rc3 is in ImagePullBackOff.",
        "Rather than checkout-web, the Degraded production app is checkout-web-staging: tag 2.14.0-rc3, ImagePullBackOff.",
        "checkout-web is Degraded, but the tag is not 2.14.0-rc3 and the pod is in ImagePullBackOff.",
        // The staging twin, and the production app with the twin's tag or reason.
        "checkout-web-staging is the Degraded production app; its pod cannot pull image tag 2.14.0-rc3 (ImagePullBackOff).",
        "checkout-web is Degraded on production: its new pod cannot pull image tag 2.14.0-rc2 (ImagePullBackOff).",
        "checkout-web is Degraded on production: its pod is in CrashLoopBackOff on image tag 2.14.0-rc3.",
        // Near-miss tags and a missing cause.
        "checkout-web is Degraded on production: its new pod cannot pull image tag 2.14.0-rc30 (ImagePullBackOff).",
        "checkout-web is Degraded on production: its new pod cannot pull image tag 2.14.0 (ImagePullBackOff).",
        "checkout-web is Degraded on production and runs tag 2.14.0-rc3.",
        // The registry cause on the staging twin or on the twin's tag.
        "checkout-web-staging is Degraded on production because the image tag 2.14.0-rc3 does not exist in the registry.",
        "checkout-web is Degraded on production because the image tag 2.14.0-rc2 does not exist in the registry.",
        // A cause the pod's events do not give.
        "checkout-web is Degraded on production: the 2.14.0-rc3 pod crash-loops because the STRIPE_KEY environment variable is missing.",
        "checkout-web is Degraded on production: the 2.14.0-rc3 pod is OOMKilled because its memory limit is too low.",
    ]);
    case.unresolved(&[
        // A cause in a wording nobody reviewed: it shares words with the pod's
        // own health message, so it is adjudicated rather than failed.
        "checkout-web is Degraded on production: the registry rejected the kubelet's expired credentials for the 2.14.0-rc3 image.",
        "checkout-web is Degraded on production and runs image tag 2.14.0-rc3.",
    ]);
    // The registry wordings are bound to the pod's waiting reason: once the pod waits for
    // another reason they no longer state it.
    let reason = "/services/argocd/resources/0d3df349-e40e-5789-bc1f-b9740a79c8a0/manifest/status/containerStatuses/0/state/waiting/reason";
    assert_ne!(
        case.outcome_after(
            reason,
            json!("CreateContainerConfigError"),
            "checkout-web is Degraded on production because the image tag 2.14.0-rc3 does not exist in the registry."
        ),
        Outcome::Pass
    );
    // The tag follows the failing pod's authored image.
    let image = "/services/argocd/resources/0d3df349-e40e-5789-bc1f-b9740a79c8a0/images/0";
    let edited = json!("ghcr.io/lumen-retail/checkout-web:2.14.0-rc4");
    assert_eq!(
        case.outcome_after(image, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            image,
            edited,
            "checkout-web is Degraded: its pod cannot pull image tag 2.14.0-rc4 (ImagePullBackOff)."
        ),
        Outcome::Pass
    );
}

#[test]
fn github_ci_failure_reads_the_failed_job_of_the_latest_run_of_the_pr_branch() {
    let case = Case::load("code/github/ci-failure-root-cause");
    case.reference_is_source_bound();
    case.passes(&[
        "Job: unit-tests\nFailing test: TestRetryBackoffCapsAtMaxDelay\nAssertion: expected delay 30s, got 32s",
        "The `unit-tests` job is red. `TestRetryBackoffCapsAtMaxDelay` (backoff_test.go:17) reports \"expected delay 30s, got 32s\".",
        "- **Job:** `unit-tests` (run 9005)\n- **Failing test:** `TestRetryBackoffCapsAtMaxDelay`\n- **Assertion:** `backoff_test.go:17: expected delay 30s, got 32s`\n\nThe `lint` and `integration-tests` jobs succeeded.",
        // A polite hedge still states the answer.
        "Probably the unit-tests job: TestRetryBackoffCapsAtMaxDelay, expected delay 30s, got 32s.",
        // A true remark about another job, in a parenthesis, is context.
        "unit-tests failed (lint was green): TestRetryBackoffCapsAtMaxDelay - expected delay 30s, got 32s",
        // The run's jobs with the conclusions the API reports, as bullets and on one line.
        "CI run 9005:\n- lint: success\n- unit-tests: failure\n- integration-tests: success\n\nFailing test: TestRetryBackoffCapsAtMaxDelay\nAssertion message: expected delay 30s, got 32s",
        "The run has three jobs: lint (success), unit-tests (failure), integration-tests (success).\nFailing test: TestRetryBackoffCapsAtMaxDelay\nAssertion message: expected delay 30s, got 32s",
        "lint ✅  unit-tests ❌  integration-tests ✅\n\nFailing test: TestRetryBackoffCapsAtMaxDelay\nAssertion message: expected delay 30s, got 32s",
        // The failed job in one clause with what passed: the other jobs, everything but
        // it, or the job itself on the earlier run.
        "All checks passed except unit-tests. Failing test: TestRetryBackoffCapsAtMaxDelay. Assertion message: expected delay 30s, got 32s",
        "unit-tests failed and the other two jobs passed. Failing test: TestRetryBackoffCapsAtMaxDelay. Assertion message: expected delay 30s, got 32s",
        "unit-tests passed on the previous run (9004) and fails on the latest (9005). Failing test: TestRetryBackoffCapsAtMaxDelay. Assertion message: expected delay 30s, got 32s",
        "The failing job is unit-tests - lint and integration-tests are both green. Failing test: TestRetryBackoffCapsAtMaxDelay. Assertion message: expected delay 30s, got 32s",
    ]);
    case.fails(&[
        // The same wordings around a job that did not fail in this run.
        "All checks passed except lint. Failing test: TestRetryBackoffCapsAtMaxDelay. Assertion message: expected delay 30s, got 32s",
        "integration-tests failed and the other two jobs passed. Failing test: TestRetryBackoffCapsAtMaxDelay. Assertion message: expected delay 30s, got 32s",
        "It was not the unit-tests job; TestRetryBackoffCapsAtMaxDelay printed \"expected delay 30s, got 32s\".",
        "Rather than unit-tests, the integration-tests job failed: TestRetryBackoffCapsAtMaxDelay, expected delay 30s, got 32s.",
        // Jobs and tests of the other authored runs.
        "The lint job failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\".",
        "The integration-tests job failed: TestRetryBackoffCapsAtMaxDelay, \"expected delay 30s, got 32s\".",
        "The unit-tests job failed in TestGatewaySandboxCapture: \"expected delay 30s, got 32s\".",
        "The unit-tests job failed in TestRetryBackoffCapsAtMaxDelay: dial tcp 10.20.4.17:8443: connect: connection reset by peer.",
        // Near-miss test name.
        "The unit-tests job failed in TestRetryBackoffCapsAtMaxDelays: \"expected delay 30s, got 32s\".",
        "The unit-tests job failed in TestRetryBackoffCapsAtMaxDelay.",
    ]);
    case.unresolved(&[
        // A negated verb after the job, and the reviewed contradiction cue
        // ("passed" where the failed job is asked for): never an assertion.
        "unit-tests did not fail; TestRetryBackoffCapsAtMaxDelay: expected delay 30s, got 32s",
        "The unit-tests job passed. TestRetryBackoffCapsAtMaxDelay: expected delay 30s, got 32s",
        "unit-tests is green. TestRetryBackoffCapsAtMaxDelay: expected delay 30s, got 32s.",
        // The run's jobs with the conclusions swapped.
        "CI run 9005:\n- lint: failure\n- unit-tests: success\n- integration-tests: success\n\nFailing test: TestRetryBackoffCapsAtMaxDelay\nAssertion message: expected delay 30s, got 32s",
        "The run has three jobs: lint (failure), unit-tests (success), integration-tests (success).\nFailing test: TestRetryBackoffCapsAtMaxDelay\nAssertion message: expected delay 30s, got 32s",
        "lint ❌  unit-tests ✅  integration-tests ✅\n\nFailing test: TestRetryBackoffCapsAtMaxDelay\nAssertion message: expected delay 30s, got 32s",
        // The message is a text fact: a paraphrase or a near-miss value shares
        // its words and is left to adjudication rather than failed.
        "unit-tests failed: TestRetryBackoffCapsAtMaxDelay expected a 30s delay but got 32s.",
        "The unit-tests job failed in TestRetryBackoffCapsAtMaxDelay: \"expected delay 30s, got 31s\".",
    ]);
    // The failing test follows the authored job log.
    let log = "/services/github/workflow_jobs/50114/log";
    let edited = json!(
        "--- FAIL: TestRetryJitterIsBounded (0.00s)\n    backoff_test.go:41: expected delay 30s, got 32s\nFAIL\n"
    );
    assert_eq!(
        case.outcome_after(log, edited.clone(), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            log,
            edited,
            "unit-tests failed: TestRetryJitterIsBounded, \"expected delay 30s, got 32s\"."
        ),
        Outcome::Pass
    );
    // A newer green run of the branch leaves no failed job to report.
    assert_eq!(
        case.outcome_after(
            "/services/github/workflow_runs/9004/run_number",
            json!(62),
            &case.reference
        ),
        Outcome::EnvironmentError
    );
}

#[test]
fn gitlab_failed_pipeline_binds_the_latest_failed_main_pipeline_to_its_job_and_column() {
    let case = Case::load("code/gitlab/diagnose-failed-pipeline");
    case.reference_is_source_bound();
    case.passes(&[
        "| Pipeline | 5520 |\n| Job | integration-tests |\n| Column | refund_reason |",
        "Pipeline #5520 failed in integration-tests: column \"refund_reason\" of relation \"refunds\" does not exist.",
        "integration-tests broke pipeline 5520. The column refund_reason does not exist.",
        // A polite hedge still states the answer.
        "I believe it is pipeline 5520, integration-tests, column refund_reason.",
    ]);
    case.fails(&[
        "The failed pipeline is not 5520; integration-tests complains about refund_reason.",
        "Rather than 5520, look at pipeline 5511: integration-tests, column refund_reason.",
        "Pipeline 5520 failed in integration-tests, but the column is not refund_reason.",
        // The older failed main pipeline, the failed branch pipeline, and
        // the job that failed in them.
        "Pipeline 5511 failed in integration-tests on column refund_reason.",
        "Pipeline 5523 failed in integration-tests on column refund_reason.",
        "Pipeline 5520 failed in unit-tests: column refund_reason does not exist.",
        // Near-miss identifiers.
        "Pipeline 5521 failed in integration-tests: column refund_reason does not exist.",
        "Pipeline 5520 failed in integration-tests: column refund_reasons does not exist.",
        "Pipeline 5520 failed in integration-tests.",
    ]);
    case.unresolved(&[
        "Pipeline 5520 failed in the integration tests job on column refund_reason.",
    ]);
    // The job name follows the authored job of the selected pipeline.
    let name = "/services/gitlab/jobs/88409/name";
    assert_eq!(
        case.outcome_after(name, json!("db-integration"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            name,
            json!("db-integration"),
            "Pipeline 5520, job db-integration, column refund_reason."
        ),
        Outcome::Pass
    );
    // Once 5520 is no longer failed, the older failed pipeline is the
    // requested one and the old pipeline ID is wrong.
    assert_ne!(
        case.outcome_after(
            "/services/gitlab/pipelines/5520/status",
            json!("success"),
            &case.reference
        ),
        Outcome::Pass
    );
}

#[test]
fn vercel_github_lookup_binds_the_live_commit_to_its_pr_and_each_role() {
    let case = Case::load("code/multi/vercel-deploy-github-pr-lookup");
    case.reference_is_source_bound();
    case.passes(&[
        "| Commit | c3a9f12 |\n| PR | #482 |\n| Author | Mateo Alvarez |\n| Approved by | Ines Duarte |\n| Merged by | Riley Park |",
        "Production serves c3a9f12e8b47 from PR #482, written by mateoalv, approved by ines-d and merged by riley-p.",
        "Commit c3a9f12, PR #482. mateoalv wrote it, ines-d approved it, riley-p merged it.",
        // A polite hedge still states the answer.
        "It appears to be c3a9f12 from PR #482, written by mateoalv, approved by ines-d and merged by riley-p.",
        // Other authored records named beside or after the answer are context.
        "Commit c3a9f12 (PR #482; the later #487 deploy failed), author mateoalv, approver ines-d, merger riley-p.",
        // The reviewer who only commented, listed among the reviews before the approver
        // or the merger is named.
        "Commit c3a9f12e8b47d05a6e21f9b3c7d84e50a1b6f2d9 is PR #482, written by mateoalv.\n\nReviews:\n- junpark: COMMENTED\n- ines-d: APPROVED\n\nMerged by riley-p.",
        "Production serves commit c3a9f12e8b47d05a6e21f9b3c7d84e50a1b6f2d9, which is PR #482. It was written by mateoalv, approved by ines-d (junpark only commented and did not approve), and merged by riley-p.",
        "Commit: c3a9f12e8b47d05a6e21f9b3c7d84e50a1b6f2d9\nPR: #482\nWritten by: mateoalv\nReviewed by: junpark (comment only), ines-d (approved)\nMerged by: riley-p",
    ]);
    case.fails(&[
        "The commit in production is not c3a9f12; PR #482 was written by mateoalv, approved by ines-d and merged by riley-p.",
        "Rather than c3a9f12, production serves 7d41c9e (PR #482 by mateoalv, approved by ines-d, merged by riley-p).",
        // The failed newer deployment and its pull request.
        "Production serves 7d41c9e from PR #487, written by junpark.",
        "Production serves c3a9f12 from PR #478, written by mateoalv, approved by ines-d and merged by riley-p.",
        // The right people in the wrong roles, and a reviewer who only commented.
        "Production serves c3a9f12 from PR #482, written by ines-d, approved by mateoalv and merged by riley-p.",
        "Commit c3a9f12, PR #482. Author: mateoalv. Approved by: riley-p. Merged by: ines-d.",
        "Commit c3a9f12, PR #482. ines-d wrote it, mateoalv approved it, riley-p merged it.",
        "Commit c3a9f12, PR #482. Author: mateoalv. Approved by: junpark. Merged by: riley-p.",
        // A true remark about that reviewer does not hide a role the reply gives them.
        "Commit c3a9f12, PR #482. Author: mateoalv. junpark commented. Approved by: junpark. Merged by: riley-p.",
        "Commit c3a9f12, PR #482. Author: mateoalv. Approved by: ines-d (junpark only commented). Merged by: junpark.",
        // Near-miss identifiers and a hash that is too short to identify a commit.
        "Commit c3a9f13, PR #482, author mateoalv, approver ines-d, merger riley-p.",
        "Commit c3a9f1, PR #482, author mateoalv, approver ines-d, merger riley-p.",
        "Commit c3a9f12, PR #483, author mateoalv, approver ines-d, merger riley-p.",
        "Commit c3a9f12, PR #482, author mateoalv, approver ines-d.",
    ]);
    case.unresolved(&[
        // A negator across a verb, and a negated verb after a name: disputed.
        // (The short hash cannot be a rejected-form slot; the reader covers it.)
        "Production is not serving c3a9f12; PR #482 was written by mateoalv, approved by ines-d and merged by riley-p.",
        "Commit c3a9f12, PR #482. mateoalv did not write it. ines-d approved, riley-p merged.",
        "Commit c3a9f12, PR #482, written by Mateo, approved by Ines and merged by Riley.",
        // The review list with the two reviewers' states swapped.
        "Commit c3a9f12e8b47d05a6e21f9b3c7d84e50a1b6f2d9 is PR #482, written by mateoalv.\n\nReviews:\n- junpark: APPROVED\n- ines-d: COMMENTED\n\nMerged by riley-p.",
    ]);
    // The served commit follows the authored deployment states.
    assert_eq!(
        case.outcome_after(
            "/services/vercel/deployments/dpl_PXw2C2jTZZ3IIez5YkG9aXqPW0Kq/readyState",
            json!("READY"),
            &case.reference
        ),
        Outcome::Fail
    );
    // The merger follows the authored pull request.
    let merged_by = "/services/github/pulls/acme-shop~1storefront#482/merged_by";
    assert_eq!(
        case.outcome_after(merged_by, json!("kofi-m"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            merged_by,
            json!("kofi-m"),
            "Commit c3a9f12, PR #482, author mateoalv, approver ines-d, merger Kofi Mensah."
        ),
        Outcome::Pass
    );
}

#[test]
fn vercel_status_binds_the_live_commit_and_the_latest_preview_failure() {
    let case = Case::load("code/vercel/storefront-production-status");
    case.reference_is_source_bound();
    case.passes(&[
        "1. Production serves c3a9f12 - \"Fix tax rounding for EU orders (#482)\".\n2. The latest preview failed with FUNCTION_SIZE_EXCEEDED.",
        "Commit: c3a9f12e8b47d05a6e21f9b3c7d84e50a1b6f2d9\nMessage: Fix tax rounding for EU orders\nPreview failure: a Serverless Function has exceeded the unzipped maximum size of 250 MB",
        // A polite hedge still states the answer.
        "Production seems to serve c3a9f12 (\"Fix tax rounding for EU orders\"); the preview hit the function size limit.",
        // Other authored records named beside or after the answer are context.
        "Production serves c3a9f12 (\"Fix tax rounding for EU orders\"), not the newer 7d41c9e build; the preview 4be1d07 failed with FUNCTION_SIZE_EXCEEDED.",
        // The error message in a reply's own words: the function was over the limit.
        "Production serves c3a9f12, \"Fix tax rounding for EU orders (#482)\". The latest preview failed because a function was over 250 MB.",
        "Production serves c3a9f12, \"Fix tax rounding for EU orders (#482)\". The latest preview failed at build: its serverless function bundle was too large for Vercel.",
        "Production is on c3a9f12 (Fix tax rounding for EU orders). The newest preview went over the function size limit.",
    ]);
    case.fails(&[
        "Production is not on c3a9f12 (\"Fix tax rounding for EU orders\"); the preview failed on function size.",
        "Rather than c3a9f12, production serves 7d41c9e \"Fix tax rounding for EU orders\"; the preview hit the function size limit.",
        // The failed newer production deployment, and another deployment's error.
        "Production serves 7d41c9e (\"Upgrade to Next.js 15.4 (#487)\"); the latest preview exceeded the function size limit.",
        "Production serves c3a9f12 (\"Upgrade to Next.js 15.4 (#487)\"); the latest preview exceeded the function size limit.",
        "Production serves c3a9f12 (\"Fix tax rounding for EU orders (#482)\"). The latest preview failed with BUILD_UTILS_SPAWN_1.",
        "Production serves c3a9f12 (\"Fix tax rounding for EU orders (#482)\"). The latest preview failed because the build timed out.",
        "Production serves 7d41c9e (\"Upgrade to Next.js 15.4 (#487)\"); the latest preview failed because a function was over 250 MB.",
        // Near-miss hash and a missing cause.
        "Production serves c3a9f13 (\"Fix tax rounding for EU orders (#482)\"); the preview hit the function size limit.",
        "Production serves c3a9f12 (\"Fix tax rounding for EU orders (#482)\").",
    ]);
    case.unresolved(&[
        "Production is not serving c3a9f12 (Fix tax rounding for EU orders); preview: FUNCTION_SIZE_EXCEEDED.",
        // Correct, but in wordings nobody reviewed.
        "Production serves c3a9f12 (the EU tax rounding fix); the latest preview failed with FUNCTION_SIZE_EXCEEDED.",
    ]);
    // The served commit follows the authored deployment states.
    let ready = "/services/vercel/deployments/dpl_PXw2C2jTZZ3IIez5YkG9aXqPW0Kq/readyState";
    assert_eq!(
        case.outcome_after(ready, json!("READY"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            ready,
            json!("READY"),
            "Production serves 7d41c9e, \"Upgrade to Next.js 15.4 (#487)\". The latest preview failed with FUNCTION_SIZE_EXCEEDED."
        ),
        Outcome::Pass
    );
}

#[test]
fn argocd_gitlab_trace_follows_the_deploy_commit_message_to_the_pipeline_and_its_failed_job() {
    let case = Case::load("code/multi/argocd-degraded-gitlab-commit");
    case.reference_is_source_bound();
    case.passes(&[
        "- App: checkout-web, failing on image tag 2.14.0-rc3\n- Pipeline: 7712, failed job: image-publish\n- Error: denied: permission_denied: The token provided does not match expected scopes (write:packages)",
        "checkout-web cannot pull tag 2.14.0-rc3. Pipeline #7712 promoted it although its image-publish job failed: the docker push was denied because GHCR_TOKEN had expired.",
        "checkout-web (2.14.0-rc3). Pipeline 7712: image-publish failed because the CI token expired; the promote job has `when: always` and pushed the deploy commit anyway.",
        // A polite hedge still states the answer.
        "checkout-web (2.14.0-rc3). I believe pipeline 7712 promoted it; image-publish failed with permission_denied.",
        // Other authored records named beside or after the answer are context.
        "checkout-web (2.14.0-rc3). Pipeline 7712: build, unit-tests and promote were fine, image-publish failed with permission_denied.",
    ]);
    case.fails(&[
        // Negated and rejected-alternative answers that still hold the keywords.
        "checkout-web (2.14.0-rc3). It was not pipeline 7712; image-publish failed with permission_denied.",
        "The Degraded app is not checkout-web. Tag 2.14.0-rc3, pipeline 7712, job image-publish, permission_denied.",
        "checkout-web (2.14.0-rc3). Rather than 7712, pipeline 7716 promoted it: image-publish, permission_denied.",
        "checkout-web (2.14.0-rc3). Pipeline 7712, job image-publish, error: permission was not denied.",
        // The other failed pipeline and its job, the green rc2 pipeline, the
        // staging twin and its tag, and the job that did run.
        "checkout-web (2.14.0-rc3). Pipeline 7716: lint failed, permission_denied.",
        "checkout-web (2.14.0-rc3). Pipeline 7698: image-publish failed, permission_denied.",
        "checkout-web (2.14.0-rc3). Pipeline 7712: the lint job failed with permission_denied.",
        "checkout-web (2.14.0-rc3). Pipeline 7712: the promote job failed with permission_denied.",
        "checkout-web-staging (2.14.0-rc3). Pipeline 7712: image-publish failed with permission_denied.",
        "checkout-web (2.14.0-rc2). Pipeline 7712: image-publish failed with permission_denied.",
        // Near-miss identifiers and a missing error.
        "checkout-web (2.14.0-rc3). Pipeline 7713: image-publish failed with permission_denied.",
        "checkout-web (2.14.0-rc3). Pipeline 77123: image-publish failed with permission_denied.",
        "checkout-web (2.14.0-rc3). Pipeline 7712: image-publish failed.",
    ]);
    case.unresolved(&[
        "checkout-web (2.14.0-rc3). Pipeline 7712: image-publish did not fail; permission_denied.",
        "checkout-web (2.14.0-rc3). Pipeline 7712: the image publish job failed with permission_denied.",
    ]);
    // The failed job follows the authored jobs of the pipeline.
    let name = "/services/gitlab/jobs/77123/name";
    assert_eq!(
        case.outcome_after(name, json!("push-image"), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            name,
            json!("push-image"),
            "checkout-web (2.14.0-rc3). Pipeline 7712: push-image failed with permission_denied."
        ),
        Outcome::Pass
    );
    // The reviewed wordings are bound to the error code the trace holds.
    let trace = "/services/gitlab/jobs/77123/trace";
    let unauthorized = json!(
        "$ docker push ghcr.io/lumen-retail/checkout-web:2.14.0-rc3\nunauthorized: authentication_required: no credentials were sent\nERROR: Job failed: exit code 1\n"
    );
    assert_eq!(
        case.outcome_after(trace, unauthorized, &case.reference),
        Outcome::EnvironmentError
    );
    let throttled = json!(
        "$ docker push ghcr.io/lumen-retail/checkout-web:2.14.0-rc3\ndenied: rate_limited: too many requests\nERROR: Job failed: exit code 1\n"
    );
    assert_ne!(
        case.outcome_after(
            trace,
            throttled.clone(),
            "checkout-web (2.14.0-rc3). Pipeline 7712: image-publish failed because the push was denied."
        ),
        Outcome::Pass
    );
    assert_eq!(
        case.outcome_after(
            trace,
            throttled,
            "checkout-web (2.14.0-rc3). Pipeline 7712: image-publish failed with rate_limited."
        ),
        Outcome::Pass
    );
    // The pipeline is the one the deploy commit's message names: pointing the
    // message at the green rc2 pipeline leaves no failed job to report.
    let message = "/services/gitlab/commits/lumen-retail~1storefront~1storefront-deploy@a23f2a7b21f788c07d95ceaeb529e54b7ae3dbcd/message";
    assert_eq!(
        case.outcome_after(
            message,
            json!("checkout-web(prod): promote 2.14.0-rc3\n\nPromoted by lumen-retail/storefront/checkout-web pipeline #7698 (tag v2.14.0-rc3).\n"),
            &case.reference
        ),
        Outcome::EnvironmentError
    );
}

#[test]
fn terraform_failure_binds_the_latest_run_to_its_apply_error_and_labelled_plan_counts() {
    let case = Case::load("code/terraform/payments-db-upgrade-failure");
    case.reference_is_source_bound();
    case.passes(&[
        "Run: run-Gy4tBHGggKx49nbH\nFailed resource: aws_db_instance.payments (InvalidParameterCombination)\nPlan: 1 to add, 1 to change, 0 to destroy",
        "run-Gy4tBHGggKx49nbH errored while applying `aws_db_instance.payments`: InvalidParameterCombination. Its plan was `Plan: 1 to add, 1 to change, 0 to destroy`; the apply got as far as 1 added, 0 changed, 0 destroyed.",
        // A polite hedge still states the answer.
        "It is probably run-Gy4tBHGggKx49nbH: aws_db_instance.payments, InvalidParameterCombination. Plan: 1 to add, 1 to change, 0 to destroy",
    ]);
    case.fails(&[
        "The failed run is not run-Gy4tBHGggKx49nbH. aws_db_instance.payments, InvalidParameterCombination. Plan: 1 to add, 1 to change, 0 to destroy",
        "Rather than run-Gy4tBHGggKx49nbH, look at run-nAAGvVVuIe2rfmNM: aws_db_instance.payments, InvalidParameterCombination. Plan: 1 to add, 1 to change, 0 to destroy",
        // The older errored prod run, the staging run's error code, and the
        // resource that was created successfully.
        "run-nAAGvVVuIe2rfmNM failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 1 to add, 1 to change, 0 to destroy",
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidDBParameterGroupState. Plan: 1 to add, 1 to change, 0 to destroy",
        "run-Gy4tBHGggKx49nbH failed applying aws_db_parameter_group.payments_pg16 with InvalidParameterCombination. Plan: 1 to add, 1 to change, 0 to destroy",
        // The right three counts under the wrong labels, another plan's
        // counts, and the apply's counts offered as the plan.
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 0 to add, 1 to change, 1 to destroy",
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 1 to add, 0 to change, 1 to destroy",
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 2 to add, 1 to change, 0 to destroy",
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 1 to add, 0 to change, 0 to destroy",
        // Near-miss identifiers.
        "run-Gy4tBHGggKx49nbJ failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 1 to add, 1 to change, 0 to destroy",
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payment with InvalidParameterCombination. Plan: 1 to add, 1 to change, 0 to destroy",
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination.",
    ]);
    case.unresolved(&[
        // The counts outside Terraform's format carry no reviewed label.
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. The plan would add 1, change 1 and destroy 0 resources.",
        "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: one to add, one to change, zero to destroy.",
    ]);
    // The plan counts follow the authored plan of the latest run.
    let destroyed = "/services/terraform/plans/plan-W6mMDz8dMca2TBnD/resource-destructions";
    assert_eq!(
        case.outcome_after(destroyed, json!(1), &case.reference),
        Outcome::Fail
    );
    assert_eq!(
        case.outcome_after(
            destroyed,
            json!(1),
            "run-Gy4tBHGggKx49nbH failed applying aws_db_instance.payments with InvalidParameterCombination. Plan: 1 to add, 1 to change, 1 to destroy"
        ),
        Outcome::Pass
    );
    // The run follows the workspace's authored latest run.
    assert_ne!(
        case.outcome_after(
            "/services/terraform/workspaces/ws-vJd7W10B1cG75Uxp/latest-run",
            json!("run-V1kS7u01JzNGAFbo"),
            &case.reference
        ),
        Outcome::Pass
    );
}
