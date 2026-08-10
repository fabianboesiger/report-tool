//! The design kit: the pieces of Aperture that more than one screen draws.
//!
//! **Nothing in here may know what a `Template`, a `Report` or a `Settings` is.** That is
//! the same constraint that makes `report-editor` a crate rather than a module, applied
//! one level up — and it is what lets a screen be read without also reading the kit. It
//! stays inside `app` rather than becoming a crate of its own because it is not shared
//! across binaries yet; the day it is, the boundary is already drawn.
//!
//! ## Membership
//!
//! One criterion: **a second caller already exists.** A component that wraps one element
//! for one screen is not an abstraction, it is a layer of indirection between the reader
//! and the markup, and this doc comment is the place that temptation gets refused. Things
//! deliberately left out, and where they live instead:
//!
//! | Left out | Why | Lives in |
//! |---|---|---|
//! | `Shell` | one caller, three lines, and it hosts `data-theme` | `main.rs` |
//! | `Split` | one caller: two [`Pane`]s in a `div` | `ui::editor` |
//! | `SearchField` | one caller for now — promote when Templates wants one | `ui::reports` |
//! | `Toggle` | one caller, the "numbered" checkbox | `ui::template_builder` |
//! | `Pill` | one caller, and it knows `Dictation` | `ui::editor` |
//! | `AddRow` | knows `NodeKind` | `ui::template_builder` |
//!
//! [`Row`] and [`List`] are here on exactly one ground: the Reports screen and the
//! Templates screen both draw a library listing. [`Tag`](Row) travels with them.

pub mod controls;
pub mod fields;
pub mod icon;
pub mod layout;

pub use controls::{Button, IconButton, NavLink, Variant};
pub use fields::{ChoiceCard, Disclosure, Group, NumberField, TextField};
pub use icon::{Glyph, Icon};
pub use layout::{
    Banner, Bar, EmptyState, List, Notice, NoticeKind, PageBody, PageHead, Pane, Row,
};
