#!/usr/bin/env python3
"""Audit `web/css/universal-inbox.css` to identify unused CSS classes.

Walks the CSS file to inventory every class selector and `@keyframes`,
scans `web/src/**/*.rs` plus the HTML entry points for class references
and `format!()` prefix patterns, then cross-references to bucket each
rule as Active, Decorative, Orphan, or Orphan chain. Writes a markdown
report to `Plans/css-audit-<date>.md`.

Pure stdlib — no external dependencies.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import date
from pathlib import Path


# ─── CSS parsing ────────────────────────────────────────────────────────────

ANIMATION_KEYWORDS = frozenset({
    "linear", "ease", "ease-in", "ease-out", "ease-in-out",
    "step-start", "step-end",
    "normal", "reverse", "alternate", "alternate-reverse",
    "forwards", "backwards", "both",
    "running", "paused",
    "infinite",
    "none", "inherit", "initial", "unset", "revert",
    "cubic-bezier", "steps", "var",
})

# Prefixes for classes managed by JS libraries at runtime — these never
# appear in Rust source but are present on DOM elements via the library's
# own scripts. CSS overrides targeting them must be considered alive.
JS_LIBRARY_PREFIXES = (
    "flatpickr-",   # flatpickr date picker (web/src/components/datepicker.rs)
    "notyf",        # notyf toast notifications (.notyf__toast etc.)
)

# At-rules that have a body which we should NOT descend into (token defs,
# plugin config, animation declarations — no class selectors inside).
AT_RULES_SKIP_BODY = frozenset({"theme", "plugin"})

# At-rules whose body we should descend into (the body contains normal
# selector rules).
AT_RULES_DESCEND = frozenset({"media", "layer", "supports", "container"})


@dataclass
class Rule:
    selector: str            # the original selector text
    line: int                # line number of the opening '{'
    classes: frozenset[str]  # every class token mentioned in the selector
    defined_class: str | None  # leftmost class in the rightmost compound
    is_decoration: bool      # single-compound, single-class, with pseudo/attr
    requires: frozenset[str]  # classes other than defined_class needed for the rule to fire
    start_byte: int = 0      # index of first non-whitespace char of selector
    end_byte: int = 0        # index just past the matching '}'


@dataclass
class Keyframes:
    name: str
    line: int
    start_byte: int = 0
    end_byte: int = 0


def strip_css_comments(text: str) -> str:
    """Replace /* ... */ blocks with same-length whitespace so byte
    positions AND line numbers stay accurate. Newlines are preserved
    verbatim; every other character becomes a space."""
    def replace(match: re.Match) -> str:
        return "".join("\n" if c == "\n" else " " for c in match.group(0))
    return re.sub(r"/\*[\s\S]*?\*/", replace, text)


def find_matching_brace(text: str, opening_idx: int) -> int:
    """Given text[opening_idx] == '{', return the index of the matching '}'
    or -1 if the file is unbalanced."""
    depth = 0
    for i in range(opening_idx, len(text)):
        c = text[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i
    return -1


# Class tokens in CSS selectors: `.foo`, `.foo-bar`, plus Tailwind's
# arbitrary-value escape form `.foo-\[arbitrary\]`. The brackets are
# backslash-escaped in CSS so they're CSS identifiers, not selectors.
_CSS_CLASS_TOKEN_RE = re.compile(r"\.((?:\\.|[\w-])+)")


def _extract_css_classes_from(text: str) -> list[str]:
    """Return class identifiers in a CSS selector fragment, with backslash
    escapes resolved (so `.icon-\\[logos--github-icon\\]` becomes
    `icon-[logos--github-icon]`)."""
    return [re.sub(r"\\(.)", r"\1", m.group(1)) for m in _CSS_CLASS_TOKEN_RE.finditer(text)]


def parse_rule_selector(selector: str, line: int) -> Rule:
    """Tokenize a CSS selector into a Rule record."""

    all_classes: set[str] = set()
    defined_class: str | None = None
    is_decoration = False
    requires: set[str] = set()

    # Comma-separated selectors share a Rule record but each contributes
    # its own class tokens.
    individual = [s.strip() for s in re.split(r",(?![^()]*\))", selector) if s.strip()]

    for sel in individual:
        # Extract classes from pseudo-functions :is(...), :has(...),
        # :not(...), :where(...). These inner classes are treated as
        # required: the rule won't fire unless the inner classes are
        # present on the matched DOM.
        pseudo_inner: set[str] = set()

        def collect_inner(match: re.Match) -> str:
            pseudo_inner.update(_extract_css_classes_from(match.group(1)))
            return ""

        cleaned = re.sub(r":(?:is|has|not|where)\(([^)]*)\)", collect_inner, sel)

        # Split into compound parts by combinators ( , >, +, ~) — any
        # whitespace acts as the descendant combinator.
        compound_parts = [p for p in re.split(r"\s*[>+~]\s*|\s+", cleaned.strip()) if p]
        if not compound_parts:
            continue

        # Walk compounds left-to-right collecting classes; record the
        # first class as the rule's "defined" class (used for grouping
        # in the report). For selectors like `.foo img` the subject has
        # no class, so falling back to the leftmost-class-in-selector
        # keeps the rule attributed to `.foo`.
        sel_all_classes: set[str] = set()
        sel_defined: str | None = None
        for part in compound_parts:
            part_classes = _extract_css_classes_from(part)
            for cls in part_classes:
                if sel_defined is None:
                    sel_defined = cls
                sel_all_classes.add(cls)
        sel_all_classes.update(pseudo_inner)

        # Decoration heuristic: single-compound selector, single class in
        # that compound, plus a pseudo (`:hover`, `::before`) or attribute
        # filter — i.e. a styling variant of one class, not a structural
        # selector.
        subject = compound_parts[-1]
        subject_classes = _extract_css_classes_from(subject)
        if (
            len(compound_parts) == 1
            and len(subject_classes) == 1
            and re.search(r":|\[", subject)
        ):
            is_decoration = True

        if sel_defined is not None and defined_class is None:
            defined_class = sel_defined

        all_classes.update(sel_all_classes)
        if defined_class is not None:
            requires.update(sel_all_classes - {defined_class})

    return Rule(
        selector=selector,
        line=line,
        classes=frozenset(all_classes),
        defined_class=defined_class,
        is_decoration=is_decoration,
        requires=frozenset(requires),
    )


def parse_css(text: str) -> tuple[list[Rule], list[Keyframes], dict[str, set[int]], set[str]]:
    """Parse a CSS file. Returns (rules, keyframes, animation_refs, utility_classes)."""

    text = strip_css_comments(text)
    rules: list[Rule] = []
    keyframes: list[Keyframes] = []
    animation_refs: dict[str, set[int]] = defaultdict(set)
    utility_classes: set[str] = set()

    # First sweep: scan for `--animate-*: keyframe-name ...;` tokens defined
    # in `@theme inline { ... }` blocks. These register Tailwind v4 animation
    # utilities, so the referenced @keyframes are live even though no
    # `animation:` declaration in a selector body names them directly.
    for theme_match in re.finditer(r"--animate-[\w-]+\s*:\s*([\w-]+)", text):
        kf_name = theme_match.group(1)
        kf_line = text.count("\n", 0, theme_match.start()) + 1
        animation_refs[kf_name].add(kf_line)

    header_buf: list[str] = []
    header_start_line: int | None = None
    header_start_byte: int | None = None
    line_no = 1
    i = 0
    n = len(text)

    while i < n:
        c = text[i]
        if c == "\n":
            line_no += 1

        if c == ";":
            header_text = "".join(header_buf).strip()
            if header_text.startswith("@"):
                # Statement-style at-rule like @import, @source, @custom-variant
                header_buf = []
                header_start_line = None
                header_start_byte = None
                i += 1
                continue

        if c == "{":
            header = "".join(header_buf).strip()
            header_buf = []
            line_at_brace = header_start_line if header_start_line is not None else line_no
            rule_start_byte = header_start_byte if header_start_byte is not None else i

            if header.startswith("@"):
                at_match = re.match(r"@([\w-]+)", header)
                kind = at_match.group(1) if at_match else ""

                if kind == "keyframes":
                    end = find_matching_brace(text, i)
                    if end < 0:
                        break
                    name_match = re.search(r"@keyframes\s+([\w-]+)", header)
                    if name_match:
                        keyframes.append(Keyframes(
                            name=name_match.group(1),
                            line=line_at_brace,
                            start_byte=rule_start_byte,
                            end_byte=end + 1,
                        ))
                    line_no += text[i:end + 1].count("\n")
                    i = end + 1
                    header_start_line = None
                    header_start_byte = None
                    continue

                if kind == "utility":
                    name_match = re.match(r"@utility\s+([\w-]+)", header)
                    if name_match:
                        utility_classes.add(name_match.group(1))
                    end = find_matching_brace(text, i)
                    if end < 0:
                        break
                    line_no += text[i:end + 1].count("\n")
                    i = end + 1
                    header_start_line = None
                    header_start_byte = None
                    continue

                if kind in AT_RULES_SKIP_BODY:
                    end = find_matching_brace(text, i)
                    if end < 0:
                        break
                    line_no += text[i:end + 1].count("\n")
                    i = end + 1
                    header_start_line = None
                    header_start_byte = None
                    continue

                if kind in AT_RULES_DESCEND:
                    # Step into the body — rules inside get the same
                    # treatment as top-level rules.
                    header_start_line = None
                    header_start_byte = None
                    i += 1
                    continue

                # Unknown at-rule with a body — skip the body to be safe.
                end = find_matching_brace(text, i)
                if end < 0:
                    break
                line_no += text[i:end + 1].count("\n")
                i = end + 1
                header_start_line = None
                header_start_byte = None
                continue

            # Regular selector rule
            end = find_matching_brace(text, i)
            if end < 0:
                break
            rule = parse_rule_selector(header, line_at_brace)
            rule.start_byte = rule_start_byte
            rule.end_byte = end + 1
            rules.append(rule)

            body = text[i:end + 1]

            # Scan body for `animation:` / `animation-name:` declarations.
            # The value can span multiple lines (CSS shorthand with several
            # comma-separated animations), so allow newlines.
            for anim_match in re.finditer(r"animation(?:-name)?\s*:\s*([^;}]+)", body):
                decl = anim_match.group(1).strip()
                for token in re.findall(r"\b([a-zA-Z_][\w-]*)\b", decl):
                    if token not in ANIMATION_KEYWORDS:
                        animation_refs[token].add(line_at_brace)

            line_no += body.count("\n")
            i = end + 1
            header_start_line = None
            header_start_byte = None
            continue

        if c == "}":
            # Closing brace of an @media/@layer/@supports we descended into
            header_start_line = None
            header_start_byte = None
        else:
            if c.strip() and header_start_line is None:
                header_start_line = line_no
                header_start_byte = i
            header_buf.append(c)

        i += 1

    return rules, keyframes, dict(animation_refs), utility_classes


# ─── Reference scanning ─────────────────────────────────────────────────────

# Class tokens are either plain identifiers or Tailwind arbitrary-value
# utilities like `icon-[lucide--inbox]` or `bg-[var(--ui-primary)]`.
_CLASS_TOKEN_RE = re.compile(r"^[a-zA-Z][\w-]*(?:\[[^\]\s]+\])?$")


def extract_class_tokens(class_str: str) -> list[str]:
    """Pull whitespace-separated class tokens from a value, ignoring
    interpolation placeholders `{...}`."""
    stripped = re.sub(r"\{[^}]*\}", " ", class_str)
    tokens = stripped.split()
    return [t for t in tokens if _CLASS_TOKEN_RE.match(t)]


def scan_rust_class_refs(repo_root: Path) -> tuple[set[str], set[str]]:
    """Walk Rust source files and extract class references + dynamic prefixes.

    The strategy is deliberately conservative: any string literal whose
    whitespace-split tokens all look like CSS class identifiers
    (`[a-zA-Z][\\w-]*`) is treated as a potential class reference.
    This catches not only direct `class: "..."` uses but also:

      - `match` arms returning class strings (`CardVariant::ApiKeys => "api-keys-card"`)
      - constants (`const FOO: &str = "some-class";`)
      - tuple returns, helper-function bodies, etc.

    Trade-off: any short, hyphen-free string (e.g. `"github"`, `"connected"`)
    will be considered alive. That's acceptable — false positives keep CSS
    in the file, while false negatives would delete still-used classes.
    """

    alive: set[str] = set()
    prefixes: set[str] = set()

    rs_files = list((repo_root / "web" / "src").rglob("*.rs"))

    for rs_path in rs_files:
        try:
            content = rs_path.read_text()
        except (UnicodeDecodeError, OSError):
            continue

        # Pattern 1: every "..."-quoted string literal.
        for match in re.finditer(r'"([^"\\]*(?:\\[\s\S][^"\\]*)*)"', content):
            s = match.group(1)
            # Skip strings with `{` interpolation — the literal tokens
            # around them are still extracted as Pattern 2 below.
            if "{" in s or "\\n" in s or "\\t" in s:
                continue
            tokens = s.split()
            if not tokens:
                continue
            # Every token must look like a class identifier; reject strings
            # that contain punctuation, URLs, paths, sentences, etc.
            if all(_CLASS_TOKEN_RE.match(t) for t in tokens):
                alive.update(tokens)

        # Pattern 2: interpolated strings — extract literal token slices
        # around `{...}` placeholders, plus the prefix patterns that drive
        # dynamic class names like `format!("color-{}", n)`.
        for match in re.finditer(r'"([^"\\]*(?:\\[\s\S][^"\\]*)*)"', content):
            s = match.group(1)
            if "{" not in s:
                continue
            for literal in re.split(r"\{[^}]*\}", s):
                for token in literal.split():
                    if _CLASS_TOKEN_RE.match(token):
                        alive.add(token)
            # Prefix patterns ending in '-' immediately before a `{`.
            # CSS variable refs (`--ui-…`) and CSS-property strings
            # (`background-color: …`) get caught here too — they're
            # filtered later by `categorize()` (only prefixes matching
            # actual CSS classes are kept).
            for prefix_match in re.finditer(r"([a-zA-Z][\w-]*-)\{", s):
                prefixes.add(prefix_match.group(1))

    return alive, prefixes


def scan_html_class_refs(repo_root: Path) -> set[str]:
    """Walk a small set of HTML entry points for class attribute references."""

    alive: set[str] = set()
    candidates = [
        repo_root / "web" / "index.html",
        repo_root / "api" / "tests" / "api" / "statics" / "index.html",
    ]
    for path in candidates:
        if not path.exists():
            continue
        content = path.read_text()
        for match in re.finditer(r'class="([^"]*)"', content):
            alive.update(extract_class_tokens(match.group(1)))
    return alive


# ─── Cross-reference ────────────────────────────────────────────────────────

@dataclass
class Buckets:
    active: list[Rule] = field(default_factory=list)
    decorative: list[Rule] = field(default_factory=list)
    orphan: list[Rule] = field(default_factory=list)
    orphan_chain: list[Rule] = field(default_factory=list)


def categorize(
    rules: list[Rule],
    alive_classes: set[str],
    prefixes: set[str],
) -> tuple[Buckets, set[str], set[str]]:
    """Bucket each rule against the alive set.

    Returns (buckets, effective_alive, classes_via_prefix).
    """

    # Resolve dynamic prefixes against CSS-defined class names so we know
    # which CSS classes count as alive-via-prefix.
    classes_via_prefix: set[str] = set()
    for rule in rules:
        for cls in rule.classes:
            for prefix in prefixes:
                if cls.startswith(prefix) and cls != prefix.rstrip("-"):
                    classes_via_prefix.add(cls)
            # JS-library prefixes: alive at runtime via the library's own JS.
            for prefix in JS_LIBRARY_PREFIXES:
                if cls.startswith(prefix):
                    classes_via_prefix.add(cls)

    effective_alive = alive_classes | classes_via_prefix

    buckets = Buckets()
    for rule in rules:
        if all(cls in effective_alive for cls in rule.classes):
            if rule.is_decoration:
                buckets.decorative.append(rule)
            else:
                buckets.active.append(rule)
        else:
            if rule.defined_class is None or rule.defined_class not in effective_alive:
                buckets.orphan.append(rule)
            else:
                buckets.orphan_chain.append(rule)

    return buckets, effective_alive, classes_via_prefix


# ─── Report ─────────────────────────────────────────────────────────────────

def render_report(
    buckets: Buckets,
    keyframes: list[Keyframes],
    animation_refs: dict[str, set[int]],
    alive_classes: set[str],
    prefixes: set[str],
    classes_via_prefix: set[str],
    utility_classes: set[str],
    css_path: Path,
    today: str,
) -> str:
    """Render the audit report as markdown."""

    keyframes_active = [k for k in keyframes if k.name in animation_refs]
    keyframes_orphan = [k for k in keyframes if k.name not in animation_refs]
    total_rules = (
        len(buckets.active)
        + len(buckets.decorative)
        + len(buckets.orphan)
        + len(buckets.orphan_chain)
    )

    out: list[str] = []
    out.append(f"# CSS Audit Report — {today}")
    out.append("")
    out.append(f"_Generated by `web/scripts/audit_css.py` against `{css_path.name}`._")
    out.append("")

    out.append("## Summary")
    out.append("")
    out.append("| Bucket | Count |")
    out.append("|---|---|")
    out.append(f"| Active (every class in selector is referenced) | {len(buckets.active)} |")
    out.append(f"| Decorative (`.foo:hover` / `.foo::before` etc. — defined class is Active) | {len(buckets.decorative)} |")
    out.append(f"| Orphan (defined class never referenced) | {len(buckets.orphan)} |")
    out.append(f"| Orphan chain (defined class Active but required parent/sibling dead) | {len(buckets.orphan_chain)} |")
    out.append(f"| `@keyframes` Active | {len(keyframes_active)} |")
    out.append(f"| `@keyframes` Orphan | {len(keyframes_orphan)} |")
    out.append(f"| **Total selector rules** | **{total_rules}** |")
    out.append("")

    # Only report prefixes that actually match at least one CSS class.
    productive_prefixes = sorted(
        p for p in prefixes
        if any(c.startswith(p) for c in classes_via_prefix)
    )
    if productive_prefixes:
        out.append("## Dynamic prefixes")
        out.append("")
        out.append("_Class names matching these prefixes are considered alive even if not statically referenced — they're emitted by `format!()` patterns in Rust._")
        out.append("")
        for prefix in productive_prefixes:
            matched = sorted(c for c in classes_via_prefix if c.startswith(prefix))
            sample = ", ".join("`" + c + "`" for c in matched[:8])
            suffix = "" if len(matched) <= 8 else f" (+ {len(matched) - 8} more)"
            out.append(f"- `{prefix}*` — {len(matched)} matched CSS classes: {sample}{suffix}")
        out.append("")

    if utility_classes:
        out.append("## `@utility`-defined classes")
        out.append("")
        for cls in sorted(utility_classes):
            out.append(f"- `.{cls}`")
        out.append("")

    out.append(f"## Orphans ({len(buckets.orphan)})")
    out.append("")
    out.append("_Defined class is never referenced by Rust or HTML. Delete candidates._")
    out.append("")
    if buckets.orphan:
        by_class: dict[str, list[Rule]] = defaultdict(list)
        for rule in buckets.orphan:
            key = rule.defined_class or "<no-class>"
            by_class[key].append(rule)
        for cls in sorted(by_class):
            cls_rules = sorted(by_class[cls], key=lambda r: r.line)
            out.append(f"### `.{cls}`")
            for r in cls_rules:
                out.append(f"- Line {r.line}: `{r.selector}`")
            out.append("")
    else:
        out.append("_None._")
        out.append("")

    out.append(f"## Orphan chains ({len(buckets.orphan_chain)})")
    out.append("")
    out.append("_Defined class is alive (used elsewhere), but a required parent/sibling class is dead, so this descendant rule never fires._")
    out.append("")
    if buckets.orphan_chain:
        for rule in sorted(buckets.orphan_chain, key=lambda r: r.line):
            dead = rule.classes - alive_classes - classes_via_prefix
            dead_str = ", ".join("`." + c + "`" for c in sorted(dead))
            out.append(f"- Line {rule.line}: `{rule.selector}` — dead required: {dead_str}")
        out.append("")
    else:
        out.append("_None._")
        out.append("")

    out.append(f"## `@keyframes` Orphans ({len(keyframes_orphan)})")
    out.append("")
    if keyframes_orphan:
        for kf in sorted(keyframes_orphan, key=lambda k: k.line):
            out.append(f"- Line {kf.line}: `@keyframes {kf.name}` — not referenced by any `animation:` declaration")
        out.append("")
    else:
        out.append("_None._")
        out.append("")

    out.append(f"## `@keyframes` Active ({len(keyframes_active)})")
    out.append("")
    if keyframes_active:
        for kf in sorted(keyframes_active, key=lambda k: k.line):
            refs = sorted(animation_refs.get(kf.name, set()))
            refs_str = ", ".join(str(r) for r in refs[:6])
            suffix = "" if len(refs) <= 6 else f" (+ {len(refs) - 6} more)"
            out.append(f"- Line {kf.line}: `@keyframes {kf.name}` — used at lines {refs_str}{suffix}")
        out.append("")

    out.append(f"## Active ({len(buckets.active)} rules) and Decorative ({len(buckets.decorative)} rules)")
    out.append("")
    out.append("_These are healthy. Bodies preserved in `{css_path}`._".replace("{css_path}", css_path.name))
    out.append("")

    return "\n".join(out) + "\n"


# ─── Main ───────────────────────────────────────────────────────────────────

def main() -> int:
    parser = argparse.ArgumentParser(
        description="Audit Universal Inbox CSS for unused classes.",
    )
    parser.add_argument("--output", "-o", type=Path, default=None,
                        help="Report destination (default: Plans/css-audit-<date>.md)")
    parser.add_argument("--repo-root", type=Path, default=None,
                        help="Repository root (defaults to script grandparent)")
    parser.add_argument("--quiet", "-q", action="store_true",
                        help="Suppress per-stage progress logging")
    parser.add_argument("--no-write", action="store_true",
                        help="Print report to stdout instead of writing the file")
    parser.add_argument("--apply", action="store_true",
                        help="Delete orphan rules from the CSS file in place. "
                             "Run the regular audit first, review the report, "
                             "then re-run with --apply.")
    args = parser.parse_args()

    script_path = Path(__file__).resolve()
    repo_root = (args.repo_root or script_path.parents[2]).resolve()

    css_path = repo_root / "web" / "css" / "universal-inbox.css"
    if not css_path.exists():
        print(f"ERROR: {css_path} not found", file=sys.stderr)
        return 2

    if not args.quiet:
        rel = css_path.relative_to(repo_root)
        print(f"Parsing {rel}…", file=sys.stderr)

    css_text = css_path.read_text()
    rules, keyframes, animation_refs, utility_classes = parse_css(css_text)

    if not args.quiet:
        print(f"  {len(rules)} selector rules, {len(keyframes)} @keyframes, "
              f"{len(utility_classes)} @utility classes", file=sys.stderr)
        print("Scanning Rust + HTML for class references…", file=sys.stderr)

    rust_classes, prefixes = scan_rust_class_refs(repo_root)
    html_classes = scan_html_class_refs(repo_root)
    alive_classes = rust_classes | html_classes | utility_classes

    if not args.quiet:
        print(f"  {len(alive_classes)} alive classes, {len(prefixes)} dynamic prefixes",
              file=sys.stderr)
        print("Categorizing…", file=sys.stderr)

    buckets, _, classes_via_prefix = categorize(rules, alive_classes, prefixes)

    if args.apply:
        keyframes_orphan = [k for k in keyframes if k.name not in animation_refs]
        deletions = [(r.start_byte, r.end_byte, "rule", r.selector) for r in buckets.orphan + buckets.orphan_chain]
        deletions.extend((k.start_byte, k.end_byte, "keyframes", "@keyframes " + k.name) for k in keyframes_orphan)

        # Expand each range to include the trailing newline (cleaner diff)
        # and the leading whitespace on the rule's first line.
        expanded: list[tuple[int, int, str, str]] = []
        for start, end, kind, name in deletions:
            # Back up to start of line (preserves indentation removal).
            line_start = css_text.rfind("\n", 0, start) + 1
            # Advance past trailing newline.
            line_end = end
            if line_end < len(css_text) and css_text[line_end:line_end + 1] == "\n":
                line_end += 1
            expanded.append((line_start, line_end, kind, name))

        # Sort descending by start so deletions don't invalidate earlier indices.
        expanded.sort(key=lambda t: t[0], reverse=True)

        new_text = css_text
        for start, end, kind, name in expanded:
            new_text = new_text[:start] + new_text[end:]

        css_path.write_text(new_text)
        deleted_lines = sum(end - start for start, end, _, _ in expanded)
        print(f"Applied {len(expanded)} deletions ({sum(1 for _, _, k, _ in expanded if k == 'rule')} rules + "
              f"{sum(1 for _, _, k, _ in expanded if k == 'keyframes')} @keyframes); "
              f"removed {deleted_lines} bytes from {css_path.relative_to(repo_root)}.",
              file=sys.stderr)

    today = date.today().isoformat()
    report = render_report(
        buckets, keyframes, animation_refs,
        alive_classes, prefixes, classes_via_prefix, utility_classes,
        css_path, today,
    )

    keyframes_orphan_count = sum(1 for k in keyframes if k.name not in animation_refs)
    summary = (
        f"{len(buckets.active)} active · {len(buckets.decorative)} decorative · "
        f"{len(buckets.orphan)} orphan · {len(buckets.orphan_chain)} orphan-chain · "
        f"{keyframes_orphan_count} orphan-keyframes"
    )

    if args.no_write:
        print(report)
        print(summary, file=sys.stderr)
    else:
        out_path = args.output or (repo_root / "Plans" / f"css-audit-{today}.md")
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(report)
        rel_out = out_path.relative_to(repo_root) if out_path.is_absolute() else out_path
        print(f"{summary} · report → {rel_out}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
