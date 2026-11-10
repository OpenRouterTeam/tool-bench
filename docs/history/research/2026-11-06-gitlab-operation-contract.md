# GitLab operation contract — 2026-11-06

Scope: primary GitLab documentation and source, pinned to **`v18.4.1-ee`** unless marked **current docs**. “Source-verified” describes implementation, not an independently documented API guarantee. No authenticated API calls or runtime tests were used; no error status codes or blanket role guarantees are inferred.

## 1. System-note bodies are not user-editable through either update route

**Source-verified:** ordinary note updates and discussion-note **body** updates use the same `update_note` helper, which authorizes `:admin_note`. `Note#editable?` returns `!system?`, and `NotePolicy` prevents `:admin_note` when the note is not editable. The discussion route therefore does not provide a bypass for editing a system note. Its separate `resolved` path rejects non-resolvable notes; `ResolvableNote#resolvable?` excludes system notes. This is user-facing update protection, not a claim that GitLab never modifies internal records.

Sources: [ordinary route][notes-api], [discussion route, including exactly-one-of body/resolved][discussions-api], [shared helper][notes-helper], [editable predicate][note-model], [policy][note-policy], [resolvable predicate][resolvable-note].

**Local contrast:** `crates/services/gitlab/src/notes.rs:409` guards ordinary updates against `system`; `update_discussion_note` at line 436 lacks that guard and can change a system-note body. It also accepts both `body` and `resolved`, unlike the versioned route.

## 2. Reply, whole-discussion resolution, and individual-note resolution differ

**Source-verified:**

- A normal reply to an existing thread appends a note; it does **not** resolve or unresolve every existing note. `Notes::BuildService` copies reply attributes and, when the discussion is already fully resolved, calls `resolve_without_save(current_user)` on the **new reply**. Thus a plain reply to a resolved thread stays resolved; it does not automatically reopen the thread. The reply receives its own resolution timestamp/resolver. This excludes quick-action commands embedded in the body.
- A discussion is resolved only when it is resolvable and none of its notes remains `to_be_resolved?`. A mixed thread is therefore unresolved.
- Whole-discussion `PUT .../discussions/:discussion_id` resolves all currently unresolved **resolvable user notes**, or unresolves all currently resolved ones. System/non-resolvable notes are excluded by the relation scopes; already-resolved notes are not re-stamped by another resolve.
- `PUT .../discussions/:discussion_id/notes/:note_id` with `resolved` acts on that **one note**, not the entire thread.

Sources: [reply building/inheritance][note-build], [reply creation and quick actions][note-create], [aggregate state and discussion updates][resolvable-discussion], [scoped and per-note mutations][resolvable-note], [API dispatch][discussions-api], [shared helper][notes-helper].

**Local contrast:** `crates/services/gitlab/src/notes.rs:595` inserts every reply with `resolved: false`, including replies to resolved threads. `resolve_discussion` at line 636 already loops over resolvable notes, but rewrites resolution metadata even for notes already in the requested state. The individual-note update is correctly a distinct operation.

## 3. Merge-time source-branch removal is supported; separate deletion is another operation

**Documented:** `PUT /projects/:id/merge_requests/:merge_request_iid/merge` accepts `should_remove_source_branch`; `true` requests removal when the MR is merged. MR creation/update instead documents `remove_source_branch`. A separate `DELETE /projects/:id/repository/branches/:branch` deletes a repository branch; that API documents exclusions for default/protected branches. A caller need not issue a separate deletion merely because removal was requested through the merge endpoint. [Versioned MR API][mr-doc], [versioned branch API][branch-doc].

**Source-verified caveats:** merging and deletion are not one unconditional atomic result. After merge, GitLab checks the requested/default removal choice and `can_remove_source_branch?`, then schedules a worker. The predicate checks source-project existence, protection/default-branch status, `push_code` ability, and whether the source head still matches the reviewed head. The worker also checks the source SHA before delegating deletion. These checks do **not** establish that any particular caller can delete a branch, or that the branch disappears before the merge response. The source project can differ from the target project. [Merge service][merge-service], [MR predicate][mr-model], [deletion worker][delete-worker].

**Inference for evaluators:** if the requested outcome is “merge and remove its source branch,” final branch absence can result from merge-time removal or a separate deletion. Requiring one particular route is a benchmark constraint, not a GitLab end-state requirement. Conversely, absence alone does not prove the caller selected the merge option.

**Local contrast:** `merge_requests.rs:1230` honors the merge option/stored preference, but deletes synchronously and only when source and target projects match. `repository.rs:1187` implements separate branch deletion. These simplified behaviors are not evidence of GitLab permission or timing guarantees.

## 4. `Closes #118`: conditional closure plus distinct history/event surfaces

**Documented:** in an MR description, `Closes #118` uses the default closing pattern and local issue-reference format. The referenced issue closes when the MR merges into the default branch, subject to the issue project's automatic-closing setting and authorization checks. Merging elsewhere, merely adding the text, or closing an MR without merging is not equivalent. Self-managed installations can customize the pattern. [Versioned issue-closing documentation][closing-doc].

**Source-verified:** `MergeRequest#closes_issues` extracts references in the project context from title, description, and commit messages only for the default-branch target. Post-merge processing schedules issue closure asynchronously, checks `:update_issue` for description-derived references, and checks autoclose eligibility; the worker invokes `Issues::CloseService` with the MR as `commit`. Therefore “the merge response returned” does not guarantee the issue's closure/history is already visible. [MR extraction][mr-model], [post-merge processing][post-merge], [issue worker][close-worker].

For a successful open-to-closed transition through this path:

- The issue close service creates an issue **activity-feed close event**, calls the status-history service, and executes close hooks/notification/todo handling. The MR post-merge service separately creates its merge activity event and status history. [Issue close service][issue-close], [event creation][event-create], [post-merge processing][post-merge].
- Crucially, in this version `SystemNoteService.change_status` delegates to `ResourceEvents::ChangeStateService`: closure/merge history is stored as a **`ResourceStateEvent`**, with the closing MR's ID recorded in `source_merge_request_id` for the issue closure. `ResourceStateEvent` identifies `StateNote` as its synthetic-note class. Do not require a newly persisted ordinary `Note` row or hard-code the mock's “closed via merge request !…” body. [Status-history implementation][system-issuables], [state-event creation][state-change], [synthetic-note mapping][state-model].
- This distinction is documented: some system notes are separate resource events and are **not part of the Notes API**. Activity-feed events, resource-state events, and ordinary/system notes are not interchangeable evidence. [Versioned Notes API, Resource events][notes-doc].

**Local contrast:** `merge_requests.rs:1275` closes open same-project issues referenced in the description when targeting the default branch, but does so synchronously, adds an ordinary `system: true` note with mock-specific text, and does **not** record an issue close activity event there. It does record an MR `accepted` event and “merged” system note. Treat these as mock representation choices/gaps, not vendor contracts.

## Current-doc differences / limits

The [current MR API][current-mr] documents later additions, including `draft` filtering in 19.0 and `author.bot` in 19.5, absent from the pinned introduction. The [current Discussions API][current-discussions] adds a note-body-normalization warning absent from the pinned introduction. These later documentation details are **not** projected back onto 18.4.1. Behavioral conclusions above rest on the pinned sources, including source-only details such as resolved-reply inheritance and asynchronous side effects. No exact side-effect count/order, HTTP rejection code, webhook delivery guarantee, or universal permission is claimed.

[notes-api]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/lib/api/notes.rb
[discussions-api]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/lib/api/discussions.rb#L152-247
[notes-helper]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/lib/api/helpers/notes_helpers.rb#L42-75
[note-model]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/models/note.rb#L473-475
[note-policy]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/policies/note_policy.rb#L12-34
[resolvable-note]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/models/concerns/resolvable_note.rb
[note-build]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/services/notes/build_service.rb#L5-65
[note-create]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/services/notes/create_service.rb#L8-46
[resolvable-discussion]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/models/concerns/resolvable_discussion.rb
[mr-doc]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/doc/api/merge_requests.md
[branch-doc]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/doc/api/branches.md#L308-345
[merge-service]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/services/merge_requests/merge_service.rb#L102-134
[mr-model]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/models/merge_request.rb
[delete-worker]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/workers/merge_requests/delete_source_branch_worker.rb
[closing-doc]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/doc/user/project/issues/managing_issues.md#L380-498
[post-merge]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/services/merge_requests/post_merge_service.rb#L15-105
[close-worker]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/workers/merge_requests/close_issue_worker.rb
[issue-close]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/services/issues/close_service.rb
[event-create]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/services/event_create_service.rb#L19-71
[system-issuables]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/services/system_notes/issuables_service.rb#L378-392
[state-change]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/services/resource_events/change_state_service.rb
[state-model]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/app/models/resource_state_event.rb#L9-39
[notes-doc]: https://gitlab.com/gitlab-org/gitlab/-/blob/v18.4.1-ee/doc/api/notes.md#L24-43
[current-mr]: https://docs.gitlab.com/api/merge_requests/
[current-discussions]: https://docs.gitlab.com/api/discussions/
