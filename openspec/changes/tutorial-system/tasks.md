# Tutorial System — Tasks

## 1. Tutorial runner in TUI (harness-controlled pacing)

- [x] 1.1 Add TutorialState struct: lesson_dir, current_lesson, total_lessons, lessons vec
- [x] 1.2 Add tutorial loading: scan .omegon/tutorial/*.md, sort by filename, parse frontmatter (title, order)
- [x] 1.3 Register /tutorial command with subcommands: next, prev, status, reset
- [x] 1.4 /tutorial (no args): load tutorial, queue first incomplete lesson as prompt
- [x] 1.5 /next: advance to next lesson, queue its content as prompt
- [x] 1.6 /prev: go back one lesson, queue its content as prompt  
- [x] 1.7 /tutorial status: show current lesson N/total, title, completion
- [x] 1.8 /tutorial reset: clear progress, restart from lesson 1

## 2. Progress persistence

- [x] 2.1 Load progress from .omegon/tutorial/progress.json on tutorial start
- [x] 2.2 Save progress on each /next advancement
- [x] 2.3 /tutorial reset clears progress.json

## 3. Tutorial lesson files (in omegon-demo repo)

- [ ] 3.1 Write 01-cockpit.md — explain instrument panel, engine panel
- [ ] 3.2 Write 02-tools.md — read files, demonstrate tool activity panel
- [ ] 3.3 Write 03-code.md — create files, run tests, watch write/bash tools
- [ ] 3.4 Write 04-memory.md — store/recall, watch memory string waves
- [ ] 3.5 Write 05-design.md — create design node, add decisions
- [ ] 3.6 Write 06-context.md — fill context, watch gradient shift
- [ ] 3.7 Write 07-focus.md — /focus toggle demonstration
- [ ] 3.8 Write 08-wrapup.md — summary, cleanup, next steps

## 4. Replace /demo with /tutorial

- [x] 4.1 Update launch_demo to clone tutorial repo and start tutorial runner
- [x] 4.2 Rename /demo → /tutorial in slash command registration
