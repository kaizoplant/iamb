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
use modalkit_ratatui::{ScrollActions, TerminalCursor, WindowOps};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::StatefulWidget,
};
use ratatui::{prelude::Widget, text::Span};
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
        RoomInfo,
        RoomView,
        SendAction,
    },
    config::{TunableValues, Tunables, UserDisplayStyle},
    message::{millis_to_datetime, Message, MessageTimeStamp, TIME_GUTTER_EMPTY_SPAN},
    util::space,
};

fn user_date_line(
    msg: &Message,
    width: usize,
    info: &RoomInfo,
    tunables: &TunableValues,
) -> Line<'static> {
    let user_id = msg.sender.as_ref();
    let Span { content: user, style: user_style } = tunables.get_user_span(user_id, info);
    let mut user = user.to_string();
    if let UserDisplayStyle::Username = tunables.username_display {
    } else {
        user.push_str(&format!(" ({})", user_id.as_str()));
    }
    user.push(' ');

    let mut date = if let MessageTimeStamp::OriginServer(ms) = msg.timestamp {
        millis_to_datetime(ms).format("%T %A, %B %d %Y").to_string()
    } else {
        String::new()
    };

    // truncate if needed
    if user.len() > width {
        date.clear();
        std::mem::drop(user.drain(width.saturating_sub(2)..));
        if width >= 2 {
            user.push_str("..");
        }
    } else if user.len() + date.len() >= width {
        let date_width = width - user.len();
        std::mem::drop(date.drain(..=date.len().saturating_sub(date_width) + 2));
        if date_width >= 2 {
            date.insert_str(0, "..");
        }
    }

    let padding = width - user.len() - date.len();

    Span::styled(user, user_style) +
        Span::raw(space(padding)) +
        Span::styled(date, Style::new().add_modifier(Modifier::BOLD))
}

/// State needed for rendering [MessageWidget].
pub struct MessageState {
    room_id: OwnedRoomId,
    room: MatrixRoom,

    message_id: OwnedEventId,

    /// The number of lines drawn with the last rendering.
    lines: usize,

    /// Contextual info about the viewport used during rendering.
    viewctx: ViewportContext<usize>,

    /// The jumplist of scroll offsets
    jumped: HistoryList<usize>,
}

impl MessageState {
    pub fn new(room: MatrixRoom, message_id: OwnedEventId, store: &mut ProgramStore) -> Self {
        let room_id = room.room_id().to_owned();

        let jumped = HistoryList::default();
        let viewctx = ViewportContext::default();

        Self {
            room_id,
            room,
            message_id,
            viewctx,
            jumped,
            lines: 0,
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

    /// Set the dimensions and placement within the terminal window for this list.
    pub fn set_term_info(&mut self, area: Rect) {
        self.viewctx.dimensions = (area.width as usize, area.height as usize);
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
        self.jumped.current() != &self.viewctx.corner
    }

    fn push_jump(&mut self) {
        self.jumped.push(self.viewctx.corner);
    }

    fn movement(
        &self,
        pos: usize,
        movement: &MoveType,
        count: &Count,
        ctx: &ProgramContext,
        info: &RoomInfo,
    ) -> Option<usize> {
        let count = ctx.resolve(count);

        match movement {
            MoveType::BufferPos(MovePosition::Beginning) => Some(0),
            MoveType::BufferPos(MovePosition::End) => {
                Some(self.lines.saturating_sub(self.viewctx.get_height()))
            },
            MoveType::FinalNonBlank(dir) |
            MoveType::FirstWord(dir) |
            MoveType::Line(dir) |
            MoveType::ScreenLine(dir) |
            MoveType::ParagraphBegin(dir) |
            MoveType::SectionBegin(dir) |
            MoveType::SectionEnd(dir) => {
                match dir {
                    MoveDir1D::Previous => Some(pos.saturating_sub(count)),
                    MoveDir1D::Next => Some(pos + count),
                }
            },

            _ => todo!(),
        }
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
            viewctx: self.viewctx.clone(),
            jumped: self.jumped.clone(),
            lines: self.lines,
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
                    EditTarget::Motion(mt, count) => {
                        self.movement(self.viewctx.corner, mt, count, ctx, info)
                    },
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
                    self.viewctx.corner = pos;
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
        let cursor = Cursor::new(self.viewctx.corner, 0);
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

                self.viewctx.corner = ngroup.leader.cursor().get_y();

                Ok(None)
            },
            CursorAction::Save(_) => {
                let reg = ctx.get_register().unwrap_or(Register::UnnamedCursorGroup);

                // Lists don't have groups; override any previously saved group.
                let cursor = Cursor::new(self.viewctx.corner, 0);

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
                    self.viewctx.corner = *pos;
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

impl ScrollActions<ProgramContext, ProgramStore, IambInfo> for MessageState {
    fn dirscroll(
        &mut self,
        dir: MoveDir2D,
        size: ScrollSize,
        count: &Count,
        ctx: &ProgramContext,
        store: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        let mut corner = self.viewctx.corner;

        let count = ctx.resolve(count);
        let height = self.viewctx.get_height();
        let mut rows = match size {
            ScrollSize::Cell => count,
            ScrollSize::HalfPage => count.saturating_mul(height) / 2,
            ScrollSize::Page => count.saturating_mul(height),
        };

        match dir {
            MoveDir2D::Up => {
                corner = corner.saturating_sub(rows);
            },
            MoveDir2D::Down => {
                corner += rows;
            },
            MoveDir2D::Left | MoveDir2D::Right => {
                let msg = "Cannot scroll vertically in message view";
                let err = EditError::Failure(msg.into());

                return Err(err);
            },
        }

        self.viewctx.corner = corner;

        Ok(None)
    }

    fn cursorpos(
        &mut self,
        pos: MovePosition,
        axis: Axis,
        ctx: &ProgramContext,
        store: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        match axis {
            Axis::Horizontal => {
                let msg = "Cannot scroll vertically in message view";
                let err = EditError::Failure(msg.into());

                Err(err)
            },
            Axis::Vertical => {
                // implement if a cursor is shown
                todo!()
            },
        }
    }

    fn linepos(
        &mut self,
        _: MovePosition,
        _: &Count,
        _: &ProgramContext,
        _: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        let msg = "Cannot scroll in message view using line numbers";
        let err = EditError::Failure(msg.into());

        Err(err)
    }
}

impl Scrollable<ProgramContext, ProgramStore, IambInfo> for MessageState {
    fn scroll(
        &mut self,
        style: &ScrollStyle,
        ctx: &ProgramContext,
        store: &mut ProgramStore,
    ) -> EditResult<EditInfo, IambInfo> {
        match style {
            ScrollStyle::Direction2D(dir, size, count) => {
                return self.dirscroll(*dir, *size, count, ctx, store);
            },
            ScrollStyle::CursorPos(pos, axis) => {
                return self.cursorpos(*pos, *axis, ctx, store);
            },
            ScrollStyle::LinePos(pos, count) => {
                return self.linepos(*pos, count, ctx, store);
            },
        }
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

        state.set_term_info(area);

        let Some(msg) = info.get_event(&state.message_id) else {
            todo!()
        };

        let mut message_tunables = settings.tunables.clone();
        message_tunables.user_gutter_width = 2;
        message_tunables.read_receipt_display = false;
        message_tunables.message_time_display = false;
        message_tunables.message_user_color = false;
        message_tunables.reaction_display = false;

        // ---

        let mut lines = vec![];

        // push header
        lines
            .push((user_date_line(msg, state.viewctx.get_width(), info, &settings.tunables), None));

        // push message
        let (txt, [mut msg_preview, mut reply_preview]) = msg.show_with_preview(
            Some(msg),
            false,
            state.viewctx.get_width(),
            info,
            &message_tunables,
        );

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

        // push reactions
        if settings.tunables.reaction_display {
            for (key, users) in info.get_reactions(&state.message_id) {
                let short = emojis::get(key).and_then(|emoji| emoji.shortcode()).or(
                    if key.chars().all(|c| c.is_ascii_alphanumeric()) {
                        Some(key)
                    } else {
                        None
                    },
                );

                let content = if settings.tunables.reaction_shortcode_display {
                    if let Some(short) = short {
                        format!("[{short} {}]", users.len())
                    } else {
                        continue;
                    }
                } else if let Some(short) = short {
                    format!("[{key} {}] ({short})", users.len())
                } else {
                    format!("[{key} {}]", users.len())
                };

                lines.push((Line::raw(""), None));
                lines.push((Line::raw(content), None));

                for id in users {
                    let user = settings.tunables.get_user_span(id, info);
                    lines.push((Span::raw("- ") + user, None));
                }
            }
        }

        // ---

        if state.viewctx.corner >= lines.len() {
            state.viewctx.corner = lines.len() - 1;
        }
        state.lines = lines.len();

        std::mem::drop(lines.drain(..state.viewctx.corner));

        let mut y = area.top();
        let x = area.left();

        let mut image_previews = vec![];
        for (txt, line_preview) in lines.into_iter().take(state.viewctx.get_height()) {
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
