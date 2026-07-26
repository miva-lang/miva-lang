# Issue Tracker Configuration

## Tracker Type: Local Markdown

Issues are tracked as markdown files under `.scratch/<feature-slug>/issues/` in this repository. Each file follows the naming convention `NN-title-slug.md` where `NN` is a zero-padded number in dependency order (blockers first).

## Remote

GitHub: `git@github.com:hunter-hongg/miva-lang.git`

## Usage

The engineering skills (`to-spec`, `to-tickets`, `triage`, `qa`) read from and write to `.scratch/<feature>/issues/`. When publishing tickets, write one file per ticket in dependency order.
