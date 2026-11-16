//! Bounded undo/redo command stacks paired with [`DocumentState`].

use std::collections::VecDeque;

use crate::entities::document::DocumentState;

use super::command::EditCommand;

pub const MAX_UNDO_DEPTH: usize = 100;

#[derive(Default)]
pub struct EditHistory {
    undo_stack: VecDeque<EditCommand>,
    redo_stack: Vec<EditCommand>,
}

impl EditHistory {
    pub fn apply(&mut self, command: EditCommand, doc: &mut DocumentState) {
        command.apply(doc);
        self.push_undo(command);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, doc: &mut DocumentState) -> bool {
        let Some(command) = self.undo_stack.pop_back() else {
            return false;
        };
        command.undo(doc);
        self.redo_stack.push(command);
        true
    }

    pub fn redo(&mut self, doc: &mut DocumentState) -> bool {
        let Some(command) = self.redo_stack.pop() else {
            return false;
        };
        command.apply(doc);
        self.push_undo(command);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    fn push_undo(&mut self, command: EditCommand) {
        if self.undo_stack.len() >= MAX_UNDO_DEPTH {
            self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(command);
    }
}
