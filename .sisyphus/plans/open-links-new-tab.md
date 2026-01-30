# Open Links in New Tabs from Message Previews

## TL;DR

> **Quick Summary**: Add `target="_blank" rel="noopener noreferrer"` to all links rendered in Slack, Gmail, Google Drive, and Google Calendar previews to ensure they open in new tabs.
> 
> **Deliverables**:
> - Modified `markdown_to_html()` function with regex post-processing for links
> - Updated ammonia sanitization in 3 Google integration preview files
> - New test cases verifying link attribute addition
> 
> **Estimated Effort**: Short (1-2 hours)
> **Parallel Execution**: YES - 2 waves
> **Critical Path**: Task 1 (markdown) → Task 3 (tests for markdown)

---

## Context

### Original Request
When clicking on a link in a Slack thread preview or a Gmail preview, it should always open the link in a new tab instead of the current one.

### Interview Summary
**Key Discussions**:
- **Scope expanded**: Include Google Drive and Google Calendar previews for consistency (user confirmed)
- **Security**: Add `rel="noopener noreferrer"` for security (user confirmed; ammonia defaults to this)
- **Test strategy**: Both automated tests (extend existing test modules) and manual verification
- **Regex robustness**: Use pattern that matches `<a href=` to avoid edge cases

**Research Findings**:
- Two rendering paths: comrak (markdown→HTML) and ammonia (HTML sanitization)
- Existing test modules: `markdown_to_html_tests` and `markdown_to_html_with_slack_references_tests`
- `regex` crate already imported in `markdown.rs`
- ammonia `link_rel` defaults to `Some("noopener noreferrer")` so we keep that

---

## Work Objectives

### Core Objective
Ensure all links within message content previews (Slack threads, Gmail messages, Google Drive comments, Google Calendar event descriptions) open in a new browser tab with proper security attributes.

### Concrete Deliverables
- `web/src/components/markdown.rs`: Modified `markdown_to_html()` function
- `web/src/components/integrations/google_mail/preview.rs`: Updated ammonia call
- `web/src/components/integrations/google_drive/preview.rs`: Updated ammonia call  
- `web/src/components/integrations/google_calendar/preview.rs`: Updated ammonia call
- New test cases in `markdown_to_html_tests` and `markdown_to_html_with_slack_references_tests`

### Definition of Done
- [ ] `cd web && just test "markdown"` passes with new link tests
- [ ] Manual: Slack thread link opens in new tab
- [ ] Manual: Gmail message link opens in new tab

### Must Have
- `target="_blank"` on all `<a>` tags in rendered content
- `rel="noopener noreferrer"` on all `<a>` tags for security
- No breaking changes to existing HTML rendering

### Must NOT Have (Guardrails)
- **DO NOT** modify the `Markdown` component (line 8-16) - only the underlying function
- **DO NOT** modify the `SlackMarkdown` component (line 29-38) - only the underlying function
- **DO NOT** change ammonia's default `link_rel` setting (it already defaults to noopener noreferrer)
- **DO NOT** add target="_blank" to non-link elements
- **DO NOT** modify links that are hardcoded in the component RSX (those already have target="_blank")
- **DO NOT** add new dependencies - use existing regex crate

---

## Verification Strategy (MANDATORY)

### Test Decision
- **Infrastructure exists**: YES (wasm_bindgen_test, just test)
- **User wants tests**: YES (extend existing tests)
- **Framework**: wasm_bindgen_test (existing pattern)

### Automated Test Approach

Each task includes test cases that verify:
1. Links get `target="_blank"` attribute added
2. Links get `rel="noopener noreferrer"` attribute added
3. Non-link HTML is unchanged
4. Existing tests still pass

### Manual Verification (Quick sanity check)

1. Start the app: `just run-all`
2. Navigate to a Slack notification with links in the thread
3. Click a link → should open in new tab
4. Navigate to a Gmail notification with links in the message
5. Click a link → should open in new tab

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately):
├── Task 1: Modify markdown_to_html() with regex post-processing
└── Task 2: Update ammonia calls in all 3 Google integration files (can be parallel with Task 1)

Wave 2 (After Wave 1):
└── Task 3: Add test cases for link handling

Final:
└── Task 4: Manual verification (after all code changes)

Critical Path: Task 1 → Task 3
Parallel Speedup: ~30% faster than sequential
```

### Dependency Matrix

| Task | Depends On | Blocks | Can Parallelize With |
|------|------------|--------|---------------------|
| 1 | None | 3 | 2 |
| 2 | None | 4 | 1 |
| 3 | 1 | 4 | None (needs Task 1 complete) |
| 4 | 1, 2, 3 | None | None (final verification) |

### Agent Dispatch Summary

| Wave | Tasks | Recommended Agents |
|------|-------|-------------------|
| 1 | 1, 2 | Run in parallel - both are quick edits |
| 2 | 3 | After Wave 1 completes |
| Final | 4 | Manual verification step |

---

## TODOs

- [ ] 1. Add target="_blank" to links in markdown_to_html()

  **What to do**:
  - In `markdown_to_html()` function, add regex post-processing after `md2html()` call
  - Use regex pattern: `<a ([^>]*href=)` → `<a target="_blank" rel="noopener noreferrer" $1`
  - Return the processed HTML instead of raw comrak output

  **Implementation details**:
  ```rust
  pub fn markdown_to_html(text: &str) -> String {
      let mut markdown_opts = Options::default();
      markdown_opts.extension.strikethrough = true;
      markdown_opts.extension.table = true;
      markdown_opts.extension.tasklist = true;
      markdown_opts.extension.shortcodes = true;
      markdown_opts.render.escape = true;

      let html = md2html(text, &markdown_opts);
      
      // Add target="_blank" and rel="noopener noreferrer" to all links
      let link_re = Regex::new(r#"<a ([^>]*href=)"#).unwrap();
      link_re
          .replace_all(&html, r#"<a target="_blank" rel="noopener noreferrer" $1"#)
          .to_string()
  }
  ```

  **Must NOT do**:
  - Do not modify the `Markdown` or `SlackMarkdown` components
  - Do not add the attributes to `<a>` tags without href (e.g., named anchors)
  - Do not compile the regex inside the function (for now it's acceptable; future optimization could use lazy_static)

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Single function modification with clear pattern, <10 lines change
  - **Skills**: None needed
    - This is a straightforward Rust edit following existing patterns in the file
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: Not needed - this is backend HTML generation, not UI styling

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Task 2)
  - **Blocks**: Task 3 (tests depend on this)
  - **Blocked By**: None (can start immediately)

  **References**:

  **Pattern References** (existing code to follow):
  - `web/src/components/markdown.rs:40-53` - `markdown_to_html_with_slack_references()` shows regex post-processing pattern with `replace_all()`
  - `web/src/components/markdown.rs:42` - Shows how to use `Regex::new()` and `unwrap()` pattern
  - `web/src/components/markdown.rs:47-52` - Shows chained `replace_all().to_string()` pattern

  **API/Type References**:
  - `web/src/components/markdown.rs:5` - `regex::Regex` import already present

  **WHY Each Reference Matters**:
  - Line 40-53 demonstrates the exact pattern: call `markdown_to_html()`, then apply regex replacements. The new code goes INSIDE `markdown_to_html()` itself but uses the same regex techniques.

  **Acceptance Criteria**:

  - [ ] Function `markdown_to_html()` returns HTML with `target="_blank" rel="noopener noreferrer"` on `<a>` tags
  - [ ] Regex only matches `<a` tags that have `href=` attribute
  - [ ] Non-link HTML elements are unchanged
  - [ ] `cd web && just check` passes (no compilation errors)

  **Commit**: YES
  - Message: `fix(web): add target=_blank to links in markdown rendering`
  - Files: `web/src/components/markdown.rs`
  - Pre-commit: `cd web && just check`

---

- [ ] 2. Update ammonia sanitization in Google integration previews

  **What to do**:
  - Replace `ammonia::clean(&html)` with `ammonia::Builder::default().set_tag_attribute_value("a", "target", "_blank").clean(&html).to_string()`
  - Apply to 3 files: google_mail, google_drive, google_calendar previews

  **Implementation details**:

  **File 1: google_mail/preview.rs (line 144)**
  ```rust
  // Before:
  let message_body = use_memo(move || ammonia::clean(&message().render_content_as_html()));
  
  // After:
  let message_body = use_memo(move || {
      ammonia::Builder::default()
          .set_tag_attribute_value("a", "target", "_blank")
          .clean(&message().render_content_as_html())
          .to_string()
  });
  ```

  **File 2: google_drive/preview.rs (line 157-158)**
  ```rust
  // Before:
  let cleaned_html_content =
      use_memo(move || html_content().as_ref().map(|html| ammonia::clean(html)));
  
  // After:
  let cleaned_html_content = use_memo(move || {
      html_content().as_ref().map(|html| {
          ammonia::Builder::default()
              .set_tag_attribute_value("a", "target", "_blank")
              .clean(html)
              .to_string()
      })
  });
  ```

  **File 3: google_calendar/preview.rs (line 78-83)**
  ```rust
  // Before:
  let sanitized_description = use_memo(move || {
      google_calendar_event()
          .description
          .as_ref()
          .map(|desc| ammonia::clean(desc))
  });
  
  // After:
  let sanitized_description = use_memo(move || {
      google_calendar_event()
          .description
          .as_ref()
          .map(|desc| {
              ammonia::Builder::default()
                  .set_tag_attribute_value("a", "target", "_blank")
                  .clean(desc)
                  .to_string()
          })
  });
  ```

  **Must NOT do**:
  - Do not change ammonia's default `link_rel` (already defaults to `Some("noopener noreferrer")`)
  - Do not modify other ammonia usages outside these 3 specific locations
  - Do not change the component structure or props

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Three similar small edits following the same pattern
  - **Skills**: None needed
    - Straightforward API usage change, no special domain knowledge required
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: Not needed - this is sanitization logic, not UI

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Task 1)
  - **Blocks**: Task 4 (manual verification)
  - **Blocked By**: None (can start immediately)

  **References**:

  **Pattern References** (existing code to follow):
  - `web/src/components/integrations/google_calendar/preview.rs:78-83` - Current ammonia usage pattern with `use_memo` and `Option::map`
  - `web/src/components/integrations/google_mail/preview.rs:144` - Current single-line ammonia usage

  **API/Type References**:
  - ammonia crate docs: `Builder::default().set_tag_attribute_value()` API
  - Note: `Builder::clean()` returns `Document`, need `.to_string()` to get `String`

  **Test References**:
  - `web/src/components/integrations/google_calendar/preview.rs:854-876` - Existing tests for `ammonia::clean()` behavior that verify HTML sanitization

  **WHY Each Reference Matters**:
  - The existing ammonia usage (line 144, 157-158, 78-83) shows the `use_memo` wrapper pattern that must be preserved
  - The google_calendar tests (lines 854-876) show that basic HTML tags are preserved, confirming our change won't break expected behavior

  **Acceptance Criteria**:

  - [ ] `ammonia::clean()` replaced with `ammonia::Builder::default().set_tag_attribute_value("a", "target", "_blank").clean().to_string()` in all 3 files
  - [ ] `cd web && just check` passes (no compilation errors)
  - [ ] Existing tests still pass: `cd web && just test "google_calendar"` (the existing HTML sanitization tests)

  **Commit**: YES
  - Message: `fix(web): add target=_blank to links in Google integration previews`
  - Files: `web/src/components/integrations/google_mail/preview.rs`, `web/src/components/integrations/google_drive/preview.rs`, `web/src/components/integrations/google_calendar/preview.rs`
  - Pre-commit: `cd web && just check`

---

- [ ] 3. Add test cases for link handling in markdown rendering

  **What to do**:
  - Add new test module `mod links` in `markdown_to_html_tests`
  - Add test case for links in `markdown_to_html_with_slack_references_tests`
  - Verify `target="_blank"` and `rel="noopener noreferrer"` are added to links

  **Implementation details**:

  Add in `markdown_to_html_tests` (after line 150, before `mod preformatted_text`):
  ```rust
  mod links {
      use super::*;
      use pretty_assertions::assert_eq;

      #[wasm_bindgen_test]
      fn test_markdown_to_html_link_has_target_blank() {
          assert_eq!(
              markdown_to_html("[Example](https://example.com)"),
              r#"<p><a target="_blank" rel="noopener noreferrer" href="https://example.com">Example</a></p>
"#
                  .to_string()
          );
      }

      #[wasm_bindgen_test]
      fn test_markdown_to_html_multiple_links() {
          let result = markdown_to_html("[One](https://one.com) and [Two](https://two.com)");
          assert!(result.contains(r#"target="_blank""#));
          assert!(result.contains(r#"rel="noopener noreferrer""#));
          // Count occurrences
          assert_eq!(result.matches(r#"target="_blank""#).count(), 2);
      }

      #[wasm_bindgen_test]
      fn test_markdown_to_html_autolink() {
          let result = markdown_to_html("<https://example.com>");
          assert!(result.contains(r#"target="_blank""#));
          assert!(result.contains(r#"rel="noopener noreferrer""#));
      }
  }
  ```

  Add test in `markdown_to_html_with_slack_references_tests` (after line 207):
  ```rust
  #[wasm_bindgen_test]
  fn test_markdown_to_html_with_slack_references_link_has_target_blank() {
      let result = markdown_to_html_with_slack_references("[Link](https://example.com)");
      assert!(result.contains(r#"target="_blank""#));
      assert!(result.contains(r#"rel="noopener noreferrer""#));
  }
  ```

  **Must NOT do**:
  - Do not modify existing tests
  - Do not test ammonia behavior here (that's covered by existing google_calendar tests)
  - Do not add tests that depend on specific comrak HTML output format beyond link attributes

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Adding test cases following existing patterns in the same file
  - **Skills**: None needed
    - Test patterns already established in the file
  - **Skills Evaluated but Omitted**:
    - No special skills needed for Rust unit tests

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 2 (after Task 1)
  - **Blocks**: Task 4 (manual verification)
  - **Blocked By**: Task 1 (tests must verify Task 1's implementation)

  **References**:

  **Pattern References** (existing code to follow):
  - `web/src/components/markdown.rs:60-71` - Test module structure with `mod text { use super::*; ... }`
  - `web/src/components/markdown.rs:64-70` - Single test function pattern with `assert_eq!` and multiline string
  - `web/src/components/markdown.rs:188-196` - Test in `markdown_to_html_with_slack_references_tests` using `assert_eq!`

  **Test References**:
  - `web/src/components/markdown.rs:56-180` - Full `markdown_to_html_tests` module structure
  - `web/src/components/markdown.rs:182-209` - Full `markdown_to_html_with_slack_references_tests` module

  **WHY Each Reference Matters**:
  - Lines 60-71 show exact module nesting and import pattern (`use super::*`, `use pretty_assertions::assert_eq`)
  - Lines 188-196 show how to test the slack references function specifically

  **Acceptance Criteria**:

  - [ ] New test module `links` exists in `markdown_to_html_tests`
  - [ ] At least 3 test cases: single link, multiple links, autolink
  - [ ] Test for `markdown_to_html_with_slack_references` verifies link attributes
  - [ ] `cd web && just test "markdown"` passes with all new tests

  **Automated Verification**:
  ```bash
  # Agent runs:
  cd web && just test "markdown"
  # Assert: All tests pass including new link tests
  # Assert: Output shows "test_markdown_to_html_link_has_target_blank" passed
  ```

  **Commit**: YES
  - Message: `test(web): add tests for link target=_blank in markdown rendering`
  - Files: `web/src/components/markdown.rs`
  - Pre-commit: `cd web && just test "markdown"`

---

- [ ] 4. Manual verification of link behavior

  **What to do**:
  - Start the application
  - Test Slack thread preview with links
  - Test Gmail message preview with links
  - Verify links open in new tabs

  **Must NOT do**:
  - Do not commit anything - this is verification only
  - Do not fix any issues found here - create new tasks if needed

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
    - Reason: Requires browser interaction and visual verification
  - **Skills**: [`playwright`]
    - `playwright`: Needed for browser automation to click links and verify new tab behavior
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: Not needed - this is verification, not design

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Final (after all code tasks)
  - **Blocks**: None (final task)
  - **Blocked By**: Tasks 1, 2, 3 (all code changes must be complete)

  **References**:

  **Documentation References**:
  - `README.md` - Application startup instructions (`just run-all`)

  **WHY Each Reference Matters**:
  - README shows how to start the full application for manual testing

  **Acceptance Criteria**:

  **For Slack preview** (using playwright skill):
  ```
  # Agent executes via playwright browser automation:
  1. Navigate to: http://localhost:8080 (or configured URL)
  2. Log in if needed
  3. Find a Slack notification with links in the thread preview
  4. Inspect link element: verify target="_blank" and rel="noopener noreferrer" attributes present
  5. Screenshot: .sisyphus/evidence/task-4-slack-link-attributes.png
  ```

  **For Gmail preview** (using playwright skill):
  ```
  # Agent executes via playwright browser automation:
  1. Navigate to a Gmail notification with links
  2. Inspect link element in message body: verify target="_blank" attribute present
  3. Screenshot: .sisyphus/evidence/task-4-gmail-link-attributes.png
  ```

  **Note**: Actual click-to-verify-new-tab may be difficult to automate. Attribute inspection is sufficient proof.

  **Commit**: NO (verification only)

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 1 | `fix(web): add target=_blank to links in markdown rendering` | markdown.rs | `cd web && just check` |
| 2 | `fix(web): add target=_blank to links in Google integration previews` | google_mail/preview.rs, google_drive/preview.rs, google_calendar/preview.rs | `cd web && just check` |
| 3 | `test(web): add tests for link target=_blank in markdown rendering` | markdown.rs | `cd web && just test "markdown"` |
| 4 | N/A | N/A | Manual verification |

---

## Success Criteria

### Verification Commands
```bash
cd web && just check           # Expected: no errors
cd web && just test "markdown" # Expected: all tests pass including new link tests
cd web && just test "google_calendar" # Expected: existing tests still pass
```

### Final Checklist
- [ ] All "Must Have" present: target="_blank" and rel="noopener noreferrer" on links
- [ ] All "Must NOT Have" absent: no component changes, no new dependencies
- [ ] All tests pass: `cd web && just test`
- [ ] Manual verification confirms new tab behavior
