//! Tutorial overlay — game-style first-play advisor.
//!
//! A TUI overlay that guides new operators through Omegon's interface.
//! Steps are compiled into the binary. The overlay renders on top of
//! the normal UI, highlights relevant areas, and advances on keypress
//! or operator action.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// Where to anchor the tutorial callout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    /// Centered in the conversation area.
    Center,
    /// Upper portion — leaves the footer/instruments visible.
    Upper,
}

/// How a step advances to the next one.
#[derive(Debug, Clone, PartialEq)]
pub enum Trigger {
    /// Press Tab to continue (passive step).
    Enter,
    /// Wait for a specific slash command (e.g. "/focus").
    Command(&'static str),
    /// Wait for any user message to be sent.
    AnyInput,
    /// Auto-send a prompt to the agent, wait for the turn to complete.
    /// The overlay shows a "watching..." indicator while the agent works.
    AutoPrompt(&'static str),
}

/// A single tutorial step.
#[derive(Debug, Clone)]
pub struct Step {
    pub title: &'static str,
    pub body: &'static str,
    pub anchor: Anchor,
    pub trigger: Trigger,
    /// Region to highlight (pulse border).
    pub highlight: Option<Highlight>,
}

/// A TUI region to visually highlight during a tutorial step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Highlight {
    InstrumentPanel,
    EnginePanel,
    InputBar,
    Dashboard,
}

// ─── Shared Act 1 steps (cockpit tour — same for both modes) ────────────────

const STEP_WELCOME_DEMO: Step = Step {
    title: "Welcome to the Omegon Demo",
    body: "You\u{2019}re in a pre-seeded Rust CLI project.\n\nIt has design nodes, an OpenSpec change with\nGiven/When/Then specs, and memory facts already\nloaded. This tour shows the full Omegon lifecycle:\nread \u{2192} design \u{2192} spec \u{2192} decompose \u{2192} verify.\n\nAbout 5 minutes. The agent does the work;\nyou watch.",
    anchor: Anchor::Center,
    trigger: Trigger::Enter,
    highlight: None,
};

const STEP_WELCOME_HANDS_ON: Step = Step {
    title: "Welcome to Omegon",
    body: "This is your AI agent cockpit.\n\nThe agent works alongside you in your terminal.\nUnlike a chatbot, it has memory, tracks\narchitecture decisions, enforces specs before\ncode, and can parallelize work in parallel\nbranches.\n\nThis tour uses YOUR project. About 5 minutes.",
    anchor: Anchor::Center,
    trigger: Trigger::Enter,
    highlight: None,
};

const STEP_ENGINE: Step = Step {
    title: "Engine Panel",
    body: "Bottom-left of your screen.\n\nThe engine panel shows:\n  \u{2022} Model and provider\n  \u{2022} Tier: Retribution (fast/cheap) \u{2192}\n         Victory (balanced) \u{2192}\n         Gloriana (deep reasoning)\n  \u{2022} Thinking level\n  \u{2022} Context capacity\n\nYou can change any of these mid-session.",
    anchor: Anchor::Upper,
    trigger: Trigger::Enter,
    highlight: Some(Highlight::EnginePanel),
};

const STEP_INSTRUMENTS: Step = Step {
    title: "Inference Instruments",
    body: "The center and right panels show live telemetry.\n\nWhile the agent works you\u{2019}ll see:\n  \u{2022} Context bar fills navy \u{2192} teal \u{2192} amber\n  \u{2022} Glitch characters during deep thinking\n  \u{2022} Memory strings pulse as facts are stored\n  \u{2022} Tool names appear with recency bars\n\nWatch these during the next steps.",
    anchor: Anchor::Upper,
    trigger: Trigger::Enter,
    highlight: Some(Highlight::InstrumentPanel),
};

const STEP_SIDEBAR: Step = Step {
    title: "Design Sidebar",
    body: "Right panel \u{2014} your design tree.\n\nEvery architectural decision, question, and\nstatus lives here. Nodes are grouped by status:\n  \u{2022} \u{2699} implementing  \u{25cf} decided\n  \u{2022} \u{25d0} exploring     \u{2715} blocked\n\nCtrl+D to navigate. Enter to focus a node\ninto the agent\u{2019}s context.\n\nWatch it update live during the next steps.",
    anchor: Anchor::Center,
    trigger: Trigger::Enter,
    highlight: Some(Highlight::Dashboard),
};

// ─── Demo mode STEPS (pre-seeded project, specific content) ─────────────────

/// Steps for the guided demo mode — run inside the bundled demo project.
/// References specific pre-seeded artifacts: output-formatting node,
/// add-validation OpenSpec change with tasks.md.
pub const STEPS_DEMO: &[Step] = &[
    // Act 1 — The Cockpit (passive)
    STEP_WELCOME_DEMO,
    STEP_ENGINE,
    STEP_INSTRUMENTS,
    STEP_SIDEBAR,

    // Act 2 — The Agent Works
    Step {
        title: "Reading the Project",
        body: "The agent will read this project\u{2019}s source\nfiles and store what it learns.\n\nWatch the instruments:\n  \u{2022} Tool bars light up as files are read\n  \u{2022} Memory strings pulse as facts are stored\n  \u{2022} The sidebar may update with context\n\nYou\u{2019}ll see three facts appear in memory.",
        anchor: Anchor::Upper,
        trigger: Trigger::AutoPrompt(
            "Read this project. Start with README.md, then src/config.rs, src/validate.rs, \
src/format.rs, and src/lib.rs. Understand what it does: it parses TOML config files and validates \
them. Then store exactly 3 memory facts using memory_store: \
(1) what this project does and its purpose, \
(2) the structure of the Config struct and what fields it validates, \
(3) what tests exist and what they cover."
        ),
        highlight: Some(Highlight::InstrumentPanel),
    },
    Step {
        title: "Exploring a Design Question",
        body: "This project has two design nodes:\n  \u{2022} output-formatting (exploring)\n  \u{2022} add-validation (decided)\n\nThe agent will explore the open question\nin output-formatting: should we support\nANSI color output?\n\nWatch the sidebar \u{2014} the node status\nwill change as research is added.",
        anchor: Anchor::Center,
        trigger: Trigger::AutoPrompt(
            "Use design_tree with action 'node' and node_id 'output-formatting' to read the design doc. \
It has an open question about ANSI color support. \
Research this question: consider terminal capability detection (TERM, NO_COLOR, COLORTERM env vars), \
common approaches in Rust CLI tools (the 'supports-color' crate pattern), and the tradeoffs. \
Then use design_tree_update with action 'add_research' to record your findings. \
Finally use action 'add_decision' to record a decision: should this project support ANSI color, and how?"
        ),
        highlight: Some(Highlight::Dashboard),
    },

    // Act 3 — The Lifecycle
    Step {
        title: "A Prepared Spec",
        body: "Look at the sidebar \u{2014} add-validation is\nmarked \u{25cf} decided and bound to an OpenSpec\nchange with Given/When/Then specs.\n\nThe agent will show you the spec and the\ntask plan that\u{2019}s ready for parallel execution.\n\nThis is the point where you\u{2019}d normally\nrun /cleave to split across branches.",
        anchor: Anchor::Center,
        trigger: Trigger::AutoPrompt(
            "Use openspec_manage with action 'get' and change_name 'add-validation' to read the full \
change: proposal, specs, and tasks. \
Then read the file ai/openspec/changes/add-validation/tasks.md directly using the bash tool. \
Explain clearly: (1) what the spec requires, (2) what the 5 tasks are, \
(3) how these would be split across parallel cleave branches, \
(4) what the operator would type to execute this: /cleave add-validation"
        ),
        highlight: None,
    },
    Step {
        title: "Your Turn: Run the Cleave",
        body: "The spec is clear. The plan is ready.\n\nNow you run it.\n\nClose this overlay (Esc), then type:\n  /cleave add-validation\n\nOmegon will split the work across parallel\nbranches, execute them simultaneously, and\nmerge the results.\n\nCome back to /tutorial after to finish.",
        anchor: Anchor::Center,
        trigger: Trigger::Enter,
        highlight: None,
    },

    // Act 4 — You're Ready
    Step {
        title: "Power Tools",
        body: "Key commands to know:\n\n  \u{2022} /model \u{2014} switch AI model or provider\n  \u{2022} /think \u{2014} adjust reasoning depth\n  \u{2022} /context \u{2014} change context window size\n  \u{2022} /focus \u{2014} hide panels, full-height chat\n  \u{2022} /calibrate \u{2014} adjust display colors\n  \u{2022} Ctrl+D \u{2014} navigate design tree\n  \u{2022} Ctrl+C \u{d7}2 \u{2014} quit",
        anchor: Anchor::Center,
        trigger: Trigger::Enter,
        highlight: None,
    },
    Step {
        title: "You\u{2019}re Ready!",
        body: "You\u{2019}ve seen the full Omegon lifecycle:\n\n  1. Read the project \u{2014} facts in memory\n  2. Explore design \u{2014} questions answered\n  3. Spec before code \u{2014} Given/When/Then\n  4. Decompose \u{2014} parallel branches\n\nRun Omegon in YOUR project to start.\nMemory persists. Design tree builds up.\nEvery session adds to what it knows.\n\n/help for all commands.",
        anchor: Anchor::Center,
        trigger: Trigger::Enter,
        highlight: None,
    },
];

// ─── Hands-on mode STEPS (user's own project, adaptive) ─────────────────────

/// Steps for hands-on mode — run in the operator's own project.
/// Prompts are adaptive: they work in any project, gracefully handle
/// empty design trees, and create real value (facts, nodes, specs).
pub const STEPS_HANDS_ON: &[Step] = &[
    // Act 1 — The Cockpit (passive)
    STEP_WELCOME_HANDS_ON,
    STEP_ENGINE,
    STEP_INSTRUMENTS,
    STEP_SIDEBAR,

    // Act 2 — The Agent Works
    Step {
        title: "Reading Your Project",
        body: "The agent will read YOUR project\u{2019}s source\nfiles and store what it learns as memory facts.\n\nThese facts persist across every session.\nNext time you open Omegon here, it already\nknows your codebase.\n\nWatch the instruments \u{2014} tools light up,\nmemory strings pulse as facts are stored.",
        anchor: Anchor::Upper,
        trigger: Trigger::AutoPrompt(
            "Read this project. Look for whatever source files exist: src/, lib.rs, main.rs, \
Cargo.toml, package.json, pyproject.toml, go.mod, README.md — whatever is here. \
Understand what it does and how it\u{2019}s structured. \
Then store exactly 3 memory facts using memory_store: \
(1) what this project does and its primary purpose, \
(2) the key code structure (main modules, important files, language/framework), \
(3) testing practices and coverage (or lack thereof). \
Be specific — these facts will be loaded in future sessions."
        ),
        highlight: Some(Highlight::InstrumentPanel),
    },
    Step {
        title: "Your Design Tree",
        body: "Design nodes track architectural decisions.\n\nThe agent will check your design tree.\nIf you have nodes with open questions, it\nwill explore one. If your tree is empty,\nit will create a first node \u{2014} a real\narchitecture overview of YOUR project.\n\nWatch the sidebar update live.",
        anchor: Anchor::Center,
        trigger: Trigger::AutoPrompt(
            "Use design_tree with action 'list' to see what design nodes exist. \
If there are nodes with open questions, pick the most interesting one and explore it: \
use action 'node' to read it, add research with your findings, then add a decision. \
If the design tree is empty, create a first node using action 'create' with: \
  - node_id: a short slug based on the project name or its main concern \
  - title: a clear description of the project\u{2019}s main architectural decision \
  - overview: 2-3 sentences about the architecture based on what you read \
  - status: 'exploring' \
Explain what you did and why this node matters for the project."
        ),
        highlight: Some(Highlight::Dashboard),
    },

    // Act 3 — The Lifecycle
    Step {
        title: "Spec Before Code",
        body: "Omegon enforces spec-before-code.\n\nEvery task starts with a Given/When/Then\nspec before any implementation begins.\nThe FSM blocks implementation until\nspecs exist.\n\nThe agent will now propose a real improvement\nfor YOUR project and write a spec for it.",
        anchor: Anchor::Center,
        trigger: Trigger::AutoPrompt(
            "Based on what you read about this project, identify ONE concrete, valuable improvement. \
Something specific that would actually help: better error handling, missing validation, \
a test gap, a missing feature. \
Use openspec_manage with action 'propose' to create a change (pick a descriptive slug). \
Then use action 'generate_spec' to write Given/When/Then scenarios for it \
(domain name can match the slug). \
Keep it focused: one clear requirement, 2-3 scenarios. \
This creates a real ai/openspec/ entry in your project."
        ),
        highlight: None,
    },
    Step {
        title: "The Full Lifecycle",
        body: "You\u{2019}ve seen three of the four lifecycle steps:\n\n  1. Memory \u{2014} facts persist across sessions\n  2. Design tree \u{2014} nodes track decisions\n  3. OpenSpec \u{2014} specs before code\n\nStep 4 is decomposition \u{2014} splitting the\nspec into parallel branches with /cleave.\n\nTo see that live on a prepared project:\n  /tutorial demo",
        anchor: Anchor::Center,
        trigger: Trigger::Enter,
        highlight: None,
    },

    // Act 4 — You're Ready
    Step {
        title: "Power Tools",
        body: "Key commands to know:\n\n  \u{2022} /model \u{2014} switch AI model or provider\n  \u{2022} /think \u{2014} adjust reasoning depth\n  \u{2022} /context \u{2014} change context window size\n  \u{2022} /focus \u{2014} hide panels, full-height chat\n  \u{2022} /calibrate \u{2014} adjust display colors\n  \u{2022} Ctrl+D \u{2014} navigate design tree\n  \u{2022} Ctrl+C \u{d7}2 \u{2014} quit",
        anchor: Anchor::Center,
        trigger: Trigger::Enter,
        highlight: None,
    },
    Step {
        title: "You\u{2019}re Ready!",
        body: "Omegon now knows your project.\n\n  \u{2022} Memory facts loaded every session\n  \u{2022} Design tree tracks your decisions\n  \u{2022} First OpenSpec change is ready\n  \u{2022} /cleave when you\u{2019}re ready to parallelize\n\nKeep asking it questions. Keep adding design\nnodes. The more context it has, the better\nit gets at helping you.\n\n/help for all commands.",
        anchor: Anchor::Center,
        trigger: Trigger::Enter,
        highlight: None,
    },
];

/// Legacy alias — hands-on is the default for direct /tutorial invocations.
pub const STEPS: &[Step] = STEPS_HANDS_ON;

/// Which option is highlighted in the project-choice widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TutorialChoice {
    Demo,
    MyProject,
}

/// Tutorial overlay state.
pub struct Tutorial {
    /// Current step index.
    current: usize,
    /// Whether the tutorial is active (visible).
    pub active: bool,
    /// Whether the current AutoPrompt step has sent its prompt.
    /// Reset to false when advancing to a new step.
    pub auto_prompt_sent: bool,
    /// Whether the project has pre-existing design tree content.
    /// When false, step 0 shows a project-choice widget instead of
    /// the normal passive welcome.
    pub has_design_tree: bool,
    /// True when running inside the bundled demo project (--tutorial flag).
    /// Selects STEPS_DEMO instead of STEPS_HANDS_ON.
    pub is_demo: bool,
    /// Current selection in the project-choice widget (step 0, empty project).
    pub choice: TutorialChoice,
    /// Set to true when the operator confirms a choice — caller reads
    /// `choice` to know which path to take, then advances the tutorial.
    pub choice_confirmed: bool,
}

impl Tutorial {
    pub fn new() -> Self {
        Self::with_context(false)
    }

    /// Create a tutorial with project context so steps can adapt.
    pub fn with_context(has_design_tree: bool) -> Self {
        Self::new_mode(has_design_tree, false)
    }

    /// Create a demo-mode tutorial (--tutorial flag, bundled project).
    pub fn new_demo(has_design_tree: bool) -> Self {
        Self::new_mode(has_design_tree, true)
    }

    fn new_mode(has_design_tree: bool, is_demo: bool) -> Self {
        Self {
            current: 0,
            active: true,
            auto_prompt_sent: false,
            has_design_tree,
            is_demo,
            choice: TutorialChoice::Demo,
            choice_confirmed: false,
        }
    }

    /// The active steps array — STEPS_DEMO in demo mode, STEPS_HANDS_ON otherwise.
    pub fn steps(&self) -> &'static [Step] {
        if self.is_demo { STEPS_DEMO } else { STEPS_HANDS_ON }
    }

    /// Whether the project-choice widget is active (step 0, empty project).
    pub fn showing_choice(&self) -> bool {
        self.current == 0 && !self.has_design_tree && !self.choice_confirmed && !self.is_demo
    }

    /// Toggle the choice selection left/right.
    pub fn toggle_choice(&mut self) {
        self.choice = match self.choice {
            TutorialChoice::Demo => TutorialChoice::MyProject,
            TutorialChoice::MyProject => TutorialChoice::Demo,
        };
    }

    /// Confirm the current choice. Caller should read `self.choice`
    /// and act accordingly (launch demo or advance normally).
    pub fn confirm_choice(&mut self) {
        self.choice_confirmed = true;
    }

    pub fn step(&self) -> &Step {
        &self.steps()[self.current]
    }

    pub fn step_index(&self) -> usize {
        self.current
    }

    pub fn total_steps(&self) -> usize {
        self.steps().len()
    }

    /// Advance to the next step. Returns false if already at the end.
    pub fn advance(&mut self) -> bool {
        if self.current < self.steps().len() - 1 {
            self.current += 1;
            self.auto_prompt_sent = false;
            true
        } else {
            self.active = false;
            false
        }
    }

    /// Go back one step.
    pub fn go_back(&mut self) -> bool {
        if self.current > 0 {
            self.current -= 1;
            self.auto_prompt_sent = false;
            true
        } else {
            false
        }
    }

    /// Mark the current AutoPrompt step's prompt as sent.
    pub fn mark_auto_prompt_sent(&mut self) {
        self.auto_prompt_sent = true;
    }

    /// Check if the current step is an AutoPrompt that hasn't been sent yet.
    pub fn pending_auto_prompt(&self) -> Option<&'static str> {
        if !self.active || self.auto_prompt_sent {
            return None;
        }
        if let Trigger::AutoPrompt(prompt) = &self.step().trigger {
            Some(prompt)
        } else {
            None
        }
    }

    /// Called when the agent finishes a turn. If the current step is an
    /// AutoPrompt that was sent, advance to the next step.
    pub fn on_agent_turn_complete(&mut self) {
        if !self.active { return; }
        if self.auto_prompt_sent {
            if let Trigger::AutoPrompt(_) = &self.step().trigger {
                self.advance();
            }
        }
    }

    /// Dismiss the tutorial.
    pub fn dismiss(&mut self) {
        self.active = false;
    }

    /// Check if a slash command matches the current step's trigger.
    pub fn check_command(&mut self, cmd: &str) -> bool {
        if !self.active { return false; }
        if let Trigger::Command(expected) = &self.step().trigger {
            if cmd == *expected {
                self.advance();
                return true;
            }
        }
        false
    }

    /// Check if any user input satisfies the current step's trigger.
    pub fn check_any_input(&mut self) -> bool {
        if !self.active { return false; }
        if self.step().trigger == Trigger::AnyInput {
            self.advance();
            return true;
        }
        false
    }

    /// Check if Enter was pressed and the current step accepts it.
    pub fn check_enter(&mut self) -> bool {
        if !self.active { return false; }
        if self.step().trigger == Trigger::Enter {
            self.advance();
            return true;
        }
        false
    }

    /// Get the highlight for the current step (if any).
    pub fn current_highlight(&self) -> Option<Highlight> {
        if !self.active { return None; }
        self.step().highlight
    }

    /// Render the tutorial overlay into the given area.
    /// `footer_height` is the actual footer height so we can position above it.
    /// Highlighting is handled by the widgets themselves — see tutorial_highlight field on App.
    pub fn render(&self, area: Rect, buf: &mut Buffer, theme: &dyn super::theme::Theme, footer_height: u16) {
        if !self.active { return; }

        // Step 0 on an empty project shows a project-choice widget
        // instead of the normal passive welcome step.
        if self.showing_choice() {
            self.render_choice(area, buf, theme, footer_height);
            return;
        }

        let step = self.step();

        // Smart positioning: avoid covering highlighted areas.
        // AutoPrompt steps that are running get a larger overlay to cover
        // the conversation chaos while the agent works behind the scenes.
        let auto_prompt_active = self.auto_prompt_sent
            && matches!(step.trigger, Trigger::AutoPrompt(_));

        let overlay = if auto_prompt_active {
            // Agent is working — large centered overlay covers the conversation
            large_centered_rect(area, footer_height)
        } else {
            match (&step.anchor, &step.highlight) {
                // Steps highlighting footer elements → position in upper area
                (_, Some(Highlight::EnginePanel | Highlight::InstrumentPanel)) => {
                    upper_rect(area, footer_height)
                }
                // Steps highlighting input → position in center-upper (above input bar)
                (_, Some(Highlight::InputBar)) => {
                    upper_rect(area, footer_height + 3) // extra 3 for input bar
                }
                // Steps highlighting dashboard → center in the conversation area.
                // Dashboard is ~40 cols wide on the right; overlay lives in the
                // conversation zone to its left, visually paired with the sidebar.
                (_, Some(Highlight::Dashboard)) => {
                    let dash_width: u16 = 40;
                    let conv_width = area.width.saturating_sub(dash_width);
                    let w = 50u16.min(conv_width.saturating_sub(4));
                    let h = area.height.saturating_sub(footer_height + 4).min(16);
                    let x = area.x + (conv_width.saturating_sub(w)) / 2;
                    let y = area.y + 2;
                    Rect { x, y, width: w, height: h }
                }
                // Center for steps with no highlight or Center anchor
                (Anchor::Center, _) => centered_rect(area),
                (Anchor::Upper, _) => upper_rect(area, footer_height),
            }
        };

        // Fill the overlay area with card background — a distinct surface
        // on top of the main bg, guaranteed theme-owned. Uses card_bg rather
        // than bg to provide subtle lift and prevent any terminal default
        // color bleed-through.
        let overlay_bg = theme.card_bg();
        for y in overlay.top()..overlay.bottom() {
            for x in overlay.left()..overlay.right() {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(x, y)) {
                    cell.reset();
                    cell.set_char(' ');
                    cell.set_bg(overlay_bg);
                    cell.set_fg(theme.fg());
                }
            }
        }

        // Build the call-to-action — prominent line inside the content
        let cta = match &step.trigger {
            Trigger::Enter => "  \u{25b6} Press Tab to continue",
            Trigger::Command("focus") => "  \u{25b6} Type /focus in the input bar below",
            Trigger::Command(cmd) => {
                return self.render_with_cta(overlay, buf, theme, &format!("  \u{25b6} Type /{cmd} in the input bar below"));
            }
            Trigger::AnyInput => "  \u{25b6} Type a message in the input bar below",
            Trigger::AutoPrompt(_) => {
                if self.auto_prompt_sent {
                    "  \u{23f3} Agent is working... watch the instruments"
                } else {
                    "  \u{25b6} Press Tab to start"
                }
            }
        };

        self.render_with_cta(overlay, buf, theme, cta);
    }

    fn render_with_cta(&self, overlay: Rect, buf: &mut Buffer, theme: &dyn super::theme::Theme, cta: &str) {
        let step = self.step();
        let progress = format!(" {}/{} ", self.current + 1, self.steps().len());
        let title_line = format!("\u{1f4d8} {} ", step.title);

        let overlay_bg = theme.card_bg();

        // Forward key hint — only shown when Tab actually does something
        let forward_hint = match &step.trigger {
            Trigger::AutoPrompt(_) if self.auto_prompt_sent => "",  // waiting for agent
            _ => "[Tab]",
        };

        let forward_label = format!(" {forward_hint} ");
        let progress_label = format!("  {progress}");
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent()).bg(overlay_bg))
            .style(Style::default().bg(overlay_bg))
            .title(Span::styled(&title_line, Style::default().fg(theme.accent()).bg(overlay_bg).bold()))
            .title_bottom(
                Line::from(vec![
                    Span::styled(
                        forward_label.as_str(),
                        Style::default().fg(theme.accent_bright()).bg(overlay_bg).bold()
                    ),
                    Span::styled(" [Esc] skip  [\u{21e7}Tab] back", Style::default().fg(theme.muted()).bg(overlay_bg)),
                    Span::styled(progress_label.as_str(), Style::default().fg(theme.muted()).bg(overlay_bg)),
                ])
            );

        let inner = block.inner(overlay);
        block.render(overlay, buf);

        // Body text + call-to-action as last line
        let body_with_cta = format!("{}\n\n{}", step.body, cta);
        let text = Paragraph::new(body_with_cta)
            .style(Style::default().fg(theme.fg()).bg(theme.card_bg()))
            .wrap(Wrap { trim: false });
        text.render(inner, buf);

        // Highlight the CTA line by coloring it accent.
        // Scan upward from the bottom to find the first non-blank line —
        // that's where the CTA actually rendered (not always the last row
        // since wrapping can shift content upward in narrow terminals).
        let mut cta_y = None;
        for y in (inner.y..inner.bottom()).rev() {
            let has_content = (inner.x..inner.right()).any(|x| {
                buf.cell(ratatui::prelude::Position::new(x, y))
                    .is_some_and(|c| c.symbol() != " ")
            });
            if has_content {
                cta_y = Some(y);
                break;
            }
        }
        if let Some(y) = cta_y {
            for x in inner.x..inner.right() {
                if let Some(cell) = buf.cell_mut(ratatui::prelude::Position::new(x, y)) {
                    if cell.symbol() != " " {
                        cell.set_fg(theme.accent_bright());
                    }
                }
            }
        }
    }

    fn render_choice(&self, area: Rect, buf: &mut Buffer, theme: &dyn super::theme::Theme, footer_height: u16) {
        let overlay = large_centered_rect(area, footer_height);
        let overlay_bg = theme.card_bg();

        // Fill background
        for y in overlay.top()..overlay.bottom() {
            for x in overlay.left()..overlay.right() {
                if let Some(cell) = buf.cell_mut(ratatui::layout::Position::new(x, y)) {
                    cell.reset();
                    cell.set_char(' ');
                    cell.set_bg(overlay_bg);
                    cell.set_fg(theme.fg());
                }
            }
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent()).bg(overlay_bg))
            .style(Style::default().bg(overlay_bg))
            .title(Span::styled(
                "\u{1f4d8} Welcome to Omegon ",
                Style::default().fg(theme.accent()).bg(overlay_bg).bold()
            ))
            .title_bottom(
                Line::from(vec![
                    Span::styled(
                        " [\u{2190}/\u{2192}] select  [Tab] confirm  [Esc] exit ",
                        Style::default().fg(theme.muted()).bg(overlay_bg)
                    ),
                ]).centered()
            );

        let inner = block.inner(overlay);
        block.render(overlay, buf);

        // Layout: intro text on top, two side-by-side option boxes below
        use ratatui::layout::{Direction, Layout, Constraint};
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // intro text
                Constraint::Min(6),    // option boxes
            ])
            .split(inner);

        // Intro line
        let intro = Paragraph::new("No existing project detected. Choose a mode:")
            .style(Style::default().fg(theme.muted()).bg(overlay_bg));
        intro.render(rows[0], buf);

        // Two option columns
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(rows[1]);

        self.render_choice_option(
            cols[0], buf, theme, overlay_bg,
            TutorialChoice::Demo,
            "\u{1f4e6} Guided demo",
            "Pre-seeded Rust project.\nFull lifecycle: read \u{2192}\ndesign \u{2192} spec \u{2192} cleave.\n\n~$0.10\u{2013}0.20 in tokens",
        );
        self.render_choice_option(
            cols[1], buf, theme, overlay_bg,
            TutorialChoice::MyProject,
            "\u{1f528} My project",
            "Use your current code.\nReads your files, stores\nfacts, creates your first\ndesign node + spec.\n\n~$0.05 in tokens",
        );
    }

    fn render_choice_option(
        &self,
        area: Rect,
        buf: &mut Buffer,
        theme: &dyn super::theme::Theme,
        overlay_bg: Color,
        option: TutorialChoice,
        title: &str,
        body: &str,
    ) {
        let selected = self.choice == option;
        let (border_style, title_style) = if selected {
            (
                Style::default().fg(theme.accent_bright()).bg(overlay_bg),
                Style::default().fg(theme.accent_bright()).bg(overlay_bg).bold(),
            )
        } else {
            (
                Style::default().fg(theme.muted()).bg(overlay_bg),
                Style::default().fg(theme.fg()).bg(overlay_bg),
            )
        };

        // Shrink area slightly for padding between options
        let padded = Rect {
            x: area.x + 1,
            y: area.y,
            width: area.width.saturating_sub(2),
            height: area.height,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(format!(" {title} "), title_style));

        let inner = block.inner(padded);
        block.render(padded, buf);

        Paragraph::new(body)
            .style(Style::default().fg(if selected { theme.fg() } else { theme.muted() }).bg(overlay_bg))
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}

/// Large centered rect — covers most of the conversation area while the
/// agent works during AutoPrompt steps. Leaves footer visible.
fn large_centered_rect(parent: Rect, footer_height: u16) -> Rect {
    let available_h = parent.height.saturating_sub(footer_height + 2);
    let w = 60u16.min(parent.width.saturating_sub(4));
    let h = available_h.min(18);
    let x = parent.x + (parent.width.saturating_sub(w)) / 2;
    let y = parent.y + (available_h.saturating_sub(h)) / 3;
    Rect::new(x, y, w, h)
}

/// Center a rect — fixed max size, always fits content.
fn centered_rect(parent: Rect) -> Rect {
    let w = 50u16.min(parent.width.saturating_sub(4));
    let h = 14u16.min(parent.height.saturating_sub(4));
    let x = parent.x + (parent.width.saturating_sub(w)) / 2;
    let y = parent.y + (parent.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

/// Rect in the upper portion of the screen — leaves footer/instruments visible.
fn upper_rect(parent: Rect, footer_height: u16) -> Rect {
    let w = 50u16.min(parent.width.saturating_sub(4));
    let h = 14u16.min(parent.height.saturating_sub(footer_height + 4));
    let x = parent.x + (parent.width.saturating_sub(w)) / 2;
    // Position in upper third of available space (above footer)
    let available = parent.height.saturating_sub(footer_height);
    let y = parent.y + (available.saturating_sub(h)) / 3;
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutorial_starts_at_step_0() {
        let tut = Tutorial::new();
        assert_eq!(tut.step_index(), 0);
        assert!(tut.active);
    }

    #[test]
    fn tutorial_advances() {
        let mut tut = Tutorial::new();
        assert!(tut.advance());
        assert_eq!(tut.step_index(), 1);
    }

    #[test]
    fn tutorial_ends_at_last_step() {
        let mut tut = Tutorial::new();
        for _ in 0..tut.steps().len() - 1 {
            assert!(tut.advance());
        }
        assert!(!tut.advance()); // at end
        assert!(!tut.active); // auto-dismisses
    }

    #[test]
    fn tutorial_go_back() {
        let mut tut = Tutorial::new();
        assert!(!tut.go_back()); // can't go back from 0
        tut.advance();
        assert!(tut.go_back());
        assert_eq!(tut.step_index(), 0);
    }

    #[test]
    fn tutorial_dismiss() {
        let mut tut = Tutorial::new();
        tut.dismiss();
        assert!(!tut.active);
    }

    #[test]
    fn check_enter_on_enter_step() {
        let mut tut = Tutorial::new();
        // Step 0 has Trigger::Enter
        assert!(tut.check_enter());
        assert_eq!(tut.step_index(), 1);
    }

    #[test]
    fn check_enter_on_command_step_does_nothing() {
        let mut tut = Tutorial::new();
        // Advance to a Command trigger step
        while tut.step().trigger == Trigger::Enter {
            tut.advance();
        }
        let idx = tut.step_index();
        assert!(!tut.check_enter());
        assert_eq!(tut.step_index(), idx); // didn't advance
    }

    #[test]
    fn check_command_on_non_command_step() {
        // check_command should be a no-op on non-Command steps
        let mut tut = Tutorial::new();
        let idx = tut.step_index();
        assert!(!tut.check_command("focus"));
        assert_eq!(tut.step_index(), idx); // didn't advance
    }

    #[test]
    fn auto_prompt_lifecycle() {
        let mut tut = Tutorial::new();
        // Find an AutoPrompt step
        while !matches!(tut.step().trigger, Trigger::AutoPrompt(_)) {
            assert!(tut.advance(), "should have an AutoPrompt step");
        }
        let idx = tut.step_index();

        // Should have pending auto-prompt
        assert!(tut.pending_auto_prompt().is_some());
        assert!(!tut.auto_prompt_sent);

        // Mark as sent
        tut.mark_auto_prompt_sent();
        assert!(tut.auto_prompt_sent);
        assert!(tut.pending_auto_prompt().is_none());

        // Agent turn complete should advance
        tut.on_agent_turn_complete();
        assert_eq!(tut.step_index(), idx + 1);
        assert!(!tut.auto_prompt_sent); // reset for new step
    }

    #[test]
    fn all_steps_have_content() {
        for (i, step) in STEPS.iter().enumerate() {
            assert!(!step.title.is_empty(), "step {i} has empty title");
            assert!(!step.body.is_empty(), "step {i} has empty body");
        }
    }

    #[test]
    fn centered_rect_fits_in_parent() {
        let parent = Rect::new(0, 0, 80, 24);
        let r = centered_rect(parent);
        assert!(r.right() <= parent.right());
        assert!(r.bottom() <= parent.bottom());
        assert!(r.width >= 10);
        assert!(r.height >= 10);
    }

    #[test]
    fn upper_rect_leaves_footer_visible() {
        let parent = Rect::new(0, 0, 80, 40);
        let footer_h = 12;
        let r = upper_rect(parent, footer_h);
        // The overlay should not overlap the footer region
        assert!(r.bottom() <= parent.height - footer_h,
            "overlay bottom {} should be above footer at {}", r.bottom(), parent.height - footer_h);
    }

    #[test]
    fn centered_rect_tiny_terminal() {
        // 20x10 terminal — overlay should still fit
        let parent = Rect::new(0, 0, 20, 10);
        let r = centered_rect(parent);
        assert!(r.right() <= parent.right());
        assert!(r.bottom() <= parent.bottom());
        assert!(r.width > 0);
        assert!(r.height > 0);
    }

    #[test]
    fn inactive_tutorial_does_not_consume_input() {
        let mut tut = Tutorial::new();
        tut.dismiss();
        assert!(!tut.check_enter());
        assert!(!tut.check_any_input());
        assert!(!tut.check_command("focus"));
        assert!(tut.current_highlight().is_none());
    }
}
