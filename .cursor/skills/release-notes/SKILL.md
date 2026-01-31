---
name: release-notes
description: Generates Git release descriptions for json-schema-rs by summarizing commits between tags. Use when the user asks for release notes, a release summary, what changed in a version, or to summarize a tag; or when preparing release text for a new version.
---

# Release notes for json-schema-rs

## When to use

Apply this skill when the user:

- Asks for release notes, a release summary, or "what's in this release"
- Wants to summarize a specific version or tag
- Is preparing release text for a new version of the library

## Resolve the tag to summarize

- **User specified a version or tag** (e.g. `v0.0.4`, `0.0.4`): Use it. Normalize to the actual tag name if needed (e.g. if tags are `v0.0.4`, use that form).
- **User did not specify**: List tags with version ordering, then ask which tag to summarize.

List tags (newest first):

```bash
git tag -l --sort=-version:refname
```

Or oldest first:

```bash
git tag -l --sort=version:refname
```

Present the list and ask the user which tag to summarize (e.g. via AskQuestion or a clear prompt).

## Commit range

- **Previous tag**: In the version-ordered list, the tag immediately *before* the chosen tag is the "previous" tag.
- **First or only tag**: If the chosen tag is the first (or only) tag, there is no previous tag; commits are from repo start through the chosen tag.
- **Range**: Collect all commits *after* the previous tag *through and including* the requested tag. In Git terms: `previous_tag..requested_tag` (excludes previous_tag, includes requested_tag). For the first tag, use the chosen tag as the range (e.g. all commits reachable from that tag).

## Gathering commits

Overview (one line per commit):

```bash
git log previous_tag..requested_tag --oneline
```

Full commit messages:

```bash
git log previous_tag..requested_tag
```

For the first tag (no previous tag):

```bash
git log requested_tag --oneline
git log requested_tag
```

Merge commits and conventional-commit style messages are both in scope; use them to understand what changed.

## Optional deep dive

If a commit message is vague or critical (e.g. "fix codegen"), run `git show <commit>` to see the diff and summarize what actually changed. Use this when it would make the release description more accurate or useful.

## Writing the release description

**Audience**: Developers considering or using the library; developers scanning "what's new" per release.

**Style**:

- Succinct and insightful; no fluff.
- Group by theme where it helps (e.g. Codegen, CLI, Docs, Fixes).
- Use bullet points or short paragraphs.

**Content**:

- What changed and why it matters to users: new behavior, fixes, breaking changes, dependency or version bumps.
- Synthesize; do not list every commit. Focus on user-facing impact and clarity.

The result should be suitable for GitHub/Git release notes and helpful to anyone evaluating or upgrading the library.

## Output format

- **Default**: Present the release notes directly in the chat. Do not write them to a file unless the user explicitly asks for a file (e.g. "write to RELEASE_NOTES.md", "save to a file").
- **Copy-pasteable**: When presenting in chat, format the release notes as easy copy-pasteable markdown (e.g. in a fenced code block with `markdown` language hint) so the user can grab the raw markdown.
