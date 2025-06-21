#![allow(unused)]
use std::ops::{Deref, DerefMut};

use matrix_sdk::{
    room::Room as MatrixRoom,
    ruma::{EventId, OwnedEventId, OwnedRoomId, RoomId},
};
use modalkit::prelude::EditInfo;
use modalkit_ratatui::{
    textbox::{TextBox, TextBoxState},
    WindowOps,
};
use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};

use crate::base::{
    IambBufferId,
    IambInfo,
    IambResult,
    MessageAction,
    ProgramContext,
    ProgramStore,
    RoomFocus,
    RoomView,
    SendAction,
};

/// State needed for rendering [MessageWidget].
pub struct MessageState {
    room_id: OwnedRoomId,
    room: MatrixRoom,

    message_id: OwnedEventId,

    tbox: TextBoxState<IambInfo>,
}

impl MessageState {
    pub fn new(room: MatrixRoom, message_id: OwnedEventId, store: &mut ProgramStore) -> Self {
        let room_id = room.room_id().to_owned();

        let buf = store.buffers.load(IambBufferId::Room(
            room_id.clone(),
            RoomView::Message(message_id.clone()),
            RoomFocus::Scrollback,
        ));
        let mut tbox = TextBoxState::new(buf);
        tbox.set_readonly(true);

        Self { room_id, room, message_id, tbox }
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

    pub fn dup(&self, store: &mut ProgramStore) -> Self {
        Self {
            room_id: self.room_id.clone(),
            room: self.room.clone(),
            message_id: self.message_id.clone(),
            tbox: self.tbox.dup(store),
        }
    }
}

impl Deref for MessageState {
    type Target = TextBoxState<IambInfo>;

    fn deref(&self) -> &Self::Target {
        return &self.tbox;
    }
}

impl DerefMut for MessageState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tbox
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
        TextBox::new().render(area, buf, &mut state.tbox);
    }
}
