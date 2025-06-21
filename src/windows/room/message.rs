#![allow(unused)]
use std::ops::{Deref, DerefMut};

use matrix_sdk::{
    room::Room as MatrixRoom,
    ruma::{EventId, OwnedEventId, OwnedRoomId, RoomId},
};
use modalkit::{
    actions::{
        Action,
        CursorAction,
        EditAction,
        Editable,
        EditorAction,
        EditorActions,
        HistoryAction,
        InsertTextAction,
        Jumpable,
        PromptAction,
        Promptable,
        Scrollable,
        SelectionAction,
    },
    editing::{
        completion::CompletionList,
        context::Resolve,
        cursor::{Cursor, CursorGroup, CursorState},
        history::HistoryList,
    },
    errors::{EditError, EditResult, UIError, UIResult},
    prelude::*,
};
use modalkit_ratatui::{TerminalCursor, WindowOps};
use ratatui::prelude::Widget;
use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
use ratatui_image::Image;

use crate::{
    base::{
        IambBufferId,
        IambId,
        IambInfo,
        IambResult,
        MessageAction,
        ProgramContext,
        ProgramStore,
        RoomFocus,
        RoomView,
        SendAction,
    }, config::Tunables, message::Message
};

/// State needed for rendering [MessageWidget].
pub struct MessageState {
    room_id: OwnedRoomId,
    room: MatrixRoom,

    message_id: OwnedEventId,

    scroll_offset: usize,
    /// The jumplist of `scroll_offset`s
    jumped: HistoryList<usize>,
}

impl MessageState {
    pub fn new(room: MatrixRoom, message_id: OwnedEventId, store: &mut ProgramStore) -> Self {
        let room_id = room.room_id().to_owned();

        let jumped = HistoryList::default();

        Self {
            room_id,
            room,
            message_id,
            jumped,
            scroll_offset: 0,
        }
    }

    pub fn refresh_room(&mut self, store: &mut ProgramStore) {
        if let Some(room) = store.application.worker.client.get_room(self.room_id()) {
            self.room = room;
        }
    }

    pub async fn message_command(
        &mut self,
        act: MessageAction,
        _: ProgramContext,
        store: &mut ProgramStore,
    ) -> IambResult<EditInfo> {
        todo!()
    }

    pub fn room(&self) -> &MatrixRoom {
        &self.room
    }

    pub fn id(&self) -> &EventId {
        &self.message_id
    }

    pub fn room_id(&self) -> &RoomId {
        &self.room_id
    }

    pub fn iamb_buffer_id(&self) -> IambBufferId {
        IambBufferId::Room(
            self.room_id.clone(),
            RoomView::Message(self.message_id.clone()),
            RoomFocus::Scrollback,
        )
    }

    fn jump_changed(&mut self) -> bool {
        self.jumped.current() != &self.scroll_offset
    }

    fn push_jump(&mut self) {
        self.jumped.push(self.scroll_offset);
    }
}

impl WindowOps<IambInfo> for MessageState {
    fn draw(&mut self, area: Rect, buf: &mut Buffer, focused: bool, store: &mut ProgramStore) {
        MessageWidget::new(store).focus(focused).render(area, buf, self)
    }

    fn dup(&self, _: &mut ProgramStore) -> Self {
        Self {
            room_id: self.room_id.clone(),
            room: self.room.clone(),
            message_id: self.message_id.clone(),
            scroll_offset: self.scroll_offset,
            jumped: self.jumped.clone(),
        }
    }

    fn close(&mut self, _: CloseFlags, _: &mut ProgramStore) -> bool {
        true
    }

    fn write(
        &mut self,
        path: Option<&str>,
        flags: WriteFlags,
        store: &mut ProgramStore,
    ) -> IambResult<EditInfo> {
        if flags.contains(WriteFlags::FORCE) {
            Ok(None)
        } else {
            Err(EditError::ReadOnly.into())
        }
    }

    fn get_completions(&self) -> Option<CompletionList> {
        None
    }

    fn get_cursor_word(&self, style: &WordStyle) -> Option<String> {
        None
    }

    fn get_selected_word(&self) -> Option<String> {
        None
    }
}

impl EditorActions<ProgramContext, ProgramStore, IambInfo> for MessageState {
    fn edit(
        &mut self,
        operation: &EditAction,
        motion: &EditTarget,
        ctx: &ProgramContext,
        store: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        let info = store.application.rooms.get_or_default(self.room_id.clone());

        match operation {
            EditAction::Motion => {
                if motion.is_jumping() {
                    self.push_jump();
                }

                let pos = match motion {
                    EditTarget::CurrentPosition | EditTarget::Selection => {
                        return Ok(None);
                    },
                    EditTarget::Boundary(rt, inc, term, count) => {
                        todo!()
                    },
                    EditTarget::CharJump(mark) | EditTarget::LineJump(mark) => {
                        let mark = ctx.resolve(mark);
                        let cursor = store.cursors.get_mark(self.iamb_buffer_id(), mark)?;

                        Some(cursor.get_y())
                    },
                    EditTarget::Motion(mt, count) => todo!(),
                    EditTarget::Range(_, _, _) => {
                        return Err(EditError::Failure("Cannot use ranges in a list".to_string()));
                    },
                    EditTarget::Search(SearchType::Char(_), _, _) => {
                        let msg = "Cannot perform character search in a list";
                        let err = EditError::Failure(msg.into());

                        return Err(err);
                    },
                    EditTarget::Search(SearchType::Regex, flip, count) => {
                        todo!()
                    },
                    EditTarget::Search(SearchType::Word(_, _), _, _) => {
                        let msg = "Cannot perform word search in a list";
                        let err = EditError::Failure(msg.into());

                        return Err(err);
                    },

                    _ => {
                        let msg = format!("Unknown editing target: {motion:?}");
                        let err = EditError::Unimplemented(msg);

                        return Err(err);
                    },
                };

                if let Some(pos) = pos {
                    self.scroll_offset = pos;
                }

                return Ok(None);
            },
            EditAction::Yank => {
                todo!()
            },

            // Everything else is a modifying action.
            EditAction::ChangeCase(_) => Err(EditError::ReadOnly),
            EditAction::ChangeNumber(_, _) => Err(EditError::ReadOnly),
            EditAction::Delete => Err(EditError::ReadOnly),
            EditAction::Format => Err(EditError::ReadOnly),
            EditAction::Indent(_) => Err(EditError::ReadOnly),
            EditAction::Join(_) => Err(EditError::ReadOnly),
            EditAction::Replace(_) => Err(EditError::ReadOnly),
        }
    }

    fn mark(
        &mut self,
        name: Mark,
        _: &ProgramContext,
        store: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        let cursor = Cursor::new(self.scroll_offset, 0);
        store.cursors.set_mark(self.iamb_buffer_id(), name, cursor);

        Ok(None)
    }

    fn complete(
        &mut self,
        _: &CompletionType,
        _: &CompletionSelection,
        _: &CompletionDisplay,
        _: &ProgramContext,
        _: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        Err(EditError::ReadOnly)
    }

    fn insert_text(
        &mut self,
        _: &InsertTextAction,
        _: &ProgramContext,
        _: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        Err(EditError::ReadOnly)
    }

    fn selection_command(
        &mut self,
        _: &SelectionAction,
        _: &ProgramContext,
        _: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        todo!()
    }

    fn history_command(
        &mut self,
        act: &HistoryAction,
        _: &ProgramContext,
        _: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        match act {
            HistoryAction::Checkpoint => Ok(None),
            HistoryAction::Undo(_) => Err(EditError::Failure("Nothing to undo".into())),
            HistoryAction::Redo(_) => Err(EditError::Failure("Nothing to redo".into())),
        }
    }

    fn cursor_command(
        &mut self,
        act: &CursorAction,
        ctx: &ProgramContext,
        store: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        let info = store.application.get_room_info(self.room_id.clone());

        match act {
            CursorAction::Close(_) => Ok(None),
            CursorAction::Rotate(_, _) => Ok(None),
            CursorAction::Split(_) => Ok(None),

            CursorAction::Restore(_) => {
                let reg = ctx.get_register().unwrap_or(Register::UnnamedCursorGroup);

                // Get saved group.
                let ngroup = store.cursors.get_group(self.iamb_buffer_id(), &reg)?;

                // Lists don't have groups; override current position.
                if self.jump_changed() {
                    self.push_jump();
                }

                self.scroll_offset = ngroup.leader.cursor().get_y();

                Ok(None)
            },
            CursorAction::Save(_) => {
                let reg = ctx.get_register().unwrap_or(Register::UnnamedCursorGroup);

                // Lists don't have groups; override any previously saved group.
                let cursor = Cursor::new(self.scroll_offset, 0);

                let group = CursorGroup {
                    leader: CursorState::Location(cursor),
                    members: vec![],
                };

                store.cursors.set_group(self.iamb_buffer_id(), reg, group)?;

                Ok(None)
            },
            _ => Err(EditError::Unimplemented(format!("Unknown action: {act:?}"))),
        }
    }
}

impl Editable<ProgramContext, ProgramStore, IambInfo> for MessageState {
    fn editor_command(
        &mut self,
        act: &EditorAction,
        ctx: &ProgramContext,
        store: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        match act {
            EditorAction::Cursor(act) => self.cursor_command(act, ctx, store),
            EditorAction::Edit(ea, et) => self.edit(&ctx.resolve(ea), et, ctx, store),
            EditorAction::History(act) => self.history_command(act, ctx, store),
            EditorAction::InsertText(act) => self.insert_text(act, ctx, store),
            EditorAction::Mark(name) => self.mark(ctx.resolve(name), ctx, store),
            EditorAction::Selection(act) => self.selection_command(act, ctx, store),

            EditorAction::Complete(_, _, _) => {
                let msg = "Nothing to complete in message view";
                let err = EditError::Failure(msg.into());

                Err(err)
            },

            _ => Err(EditError::Unimplemented(format!("Unknown action: {act:?}"))),
        }
    }
}

impl Jumpable<ProgramContext, IambInfo> for MessageState {
    fn jump(
        &mut self,
        list: PositionList,
        dir: MoveDir1D,
        count: usize,
        _: &ProgramContext,
    ) -> UIResult<usize, IambInfo> {
        match list {
            PositionList::ChangeList => {
                let msg = "No changes to jump to within the list";
                let err = UIError::Failure(msg.into());

                Err(err)
            },
            PositionList::JumpList => {
                let (len, pos) = match dir {
                    MoveDir1D::Previous => {
                        if self.jumped.future_len() == 0 && self.jump_changed() {
                            // Push current position if this is the first jump backwards.
                            self.push_jump();
                        }

                        let plen = self.jumped.past_len();
                        let pos = self.jumped.prev(count);

                        (plen, pos)
                    },
                    MoveDir1D::Next => {
                        let flen = self.jumped.future_len();
                        let pos = self.jumped.next(count);

                        (flen, pos)
                    },
                };

                if len > 0 {
                    self.scroll_offset = *pos;
                }

                Ok(count.saturating_sub(len))
            },
        }
    }
}

impl Promptable<ProgramContext, ProgramStore, IambInfo> for MessageState {
    fn prompt(
        &mut self,
        act: &PromptAction,
        ctx: &ProgramContext,
        store: &mut ProgramStore,
    ) -> EditResult<Vec<(Action<IambInfo>, ProgramContext)>, IambInfo> {
        todo!()
    }
}

impl Scrollable<ProgramContext, ProgramStore, IambInfo> for MessageState {
    fn scroll(
        &mut self,
        style: &ScrollStyle,
        ctx: &ProgramContext,
        store: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        todo!()
    }
}

impl TerminalCursor for MessageState {
    fn get_term_cursor(&self) -> Option<(u16, u16)> {
        None
    }
}

/// [StatefulWidget] for Matrix messages.
pub struct MessageWidget<'a> {
    store: &'a mut ProgramStore,
    focused: bool,
}

impl<'a> MessageWidget<'a> {
    pub fn new(store: &'a mut ProgramStore) -> Self {
        Self { focused: false, store }
    }

    pub fn focus(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl StatefulWidget for MessageWidget<'_> {
    type State = MessageState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let info = self.store.application.rooms.get_or_default(state.room_id.clone());
        let settings = &self.store.application.settings;

        let height = area.height as usize;
        let width = area.width as usize;

        let Some(item) = info.get_event(&state.message_id) else {
            todo!()
        };

        let mut message_tunables = settings.tunables.clone();
        message_tunables.user_gutter_width = 2;
        message_tunables.read_receipt_display = false;

        // ---

        let mut lines = vec![];

        let (txt, [mut msg_preview, mut reply_preview]) =
            item.show_with_preview(Some(item), false, width, info, &message_tunables);

        for (row, line) in txt.lines.into_iter().enumerate() {
            // Only take the preview into the matching row number.
            // `reply` and `msg` previews are on rows,
            // so an `or` works to pick the one that matches (if any)
            let line_preview = match msg_preview {
                Some((_, _, y)) if y as usize == row => msg_preview.take(),
                _ => None,
            }
            .or(match reply_preview {
                Some((_, _, y)) if y as usize == row => reply_preview.take(),
                _ => None,
            });

            lines.push((line, line_preview));
        }

        // ---

        std::mem::drop(lines.drain(..state.scroll_offset));

        let mut y = area.top();
        let x = area.left();

        let mut image_previews = vec![];
        for (txt, line_preview) in lines.into_iter() {
            let _ = buf.set_line(x, y, &txt, area.width);
            if let Some((backend, msg_x, _)) = line_preview {
                image_previews.push((x + msg_x, y, backend));
            }

            y += 1;
        }
        // Render image previews after all text lines have been drawn, as the render might draw below the current
        // line.
        for (x, y, backend) in image_previews {
            let image_widget = Image::new(backend);
            let mut rect = backend.area();
            rect.x = x;
            rect.y = y;
            // Don't render outside of scrollback area
            if rect.bottom() <= area.bottom() && rect.right() <= area.right() {
                image_widget.render(rect, buf);
            }
        }
    }
}
