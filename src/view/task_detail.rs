use super::*;
use quire_core::core::organizer::ListId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TaskDetailSection {
    Date,
    Priority,
    Repeat,
    List,
}

fn timestamp_label(seconds: i64) -> String {
    let day = quire_core::core::date::to_iso(seconds.div_euclid(86_400));
    let time = seconds.rem_euclid(86_400);
    format!("{day} {:02}:{:02} UTC", time / 3600, time % 3600 / 60)
}

impl Workspace {
    pub(super) fn smoke_task_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous = (
            self.organizer_open,
            self.organizer_tasks,
            self.selected_note,
            self.selected_task,
            self.selected_task_list,
            self.selected_task_smart_view,
            window.focused(cx),
        );
        let id = self.session.create_task("任务详情交互 smoke".into());
        self.organizer_open = true;
        self.organizer_tasks = true;
        self.task_multi_select = false;
        self.select_task(id, window, cx);
        self.note_title_draft = "切换属性保留标题".into();
        self.note_body_draft = "切换属性保留描述".into();
        let right = KeyDownEvent {
            keystroke: Keystroke::parse("right").unwrap(),
            is_held: false,
        };
        let enter = KeyDownEvent {
            keystroke: Keystroke::parse("enter").unwrap(),
            is_held: false,
        };
        let escape = KeyDownEvent {
            keystroke: Keystroke::parse("escape").unwrap(),
            is_held: false,
        };
        self.toggle_task_detail_section(TaskDetailSection::Priority, window, cx);
        assert!(self.task_detail_focus.is_focused(window));
        let frame = self
            .render_task_detail(false, cx)
            .expect("selected task detail");
        drop(frame);
        self.task_detail_key_down(&right, window, cx);
        self.task_detail_key_down(&enter, window, cx);
        let task = self.session.organizer.task(id).unwrap();
        assert_eq!(task.priority, Priority::Low);
        assert_eq!(task.title, "切换属性保留标题");
        assert_eq!(task.notes, "切换属性保留描述");
        assert!(self.task_detail_section.is_none());
        self.toggle_task_detail_section(TaskDetailSection::Repeat, window, cx);
        let frame = self.render_task_detail(false, cx).unwrap();
        drop(frame);
        self.task_detail_key_down(&right, window, cx);
        self.task_detail_key_down(&enter, window, cx);
        assert_eq!(
            self.session.organizer.task(id).unwrap().repeat,
            Repeat::Daily
        );
        let focus_before_date = window.focused(cx);
        self.toggle_task_detail_section(TaskDetailSection::Date, window, cx);
        assert!(self.task_due_input.read(cx).focus.is_focused(window));
        let frame = self.render_task_detail(true, cx).unwrap();
        drop(frame);
        self.set_detail_date("不是日期".into(), window, cx);
        assert!(self.task_due_error);
        assert!(self.session.organizer.task(id).unwrap().due.is_none());
        self.set_detail_date("2026-10-20".into(), window, cx);
        assert!(!self.task_due_error);
        assert_eq!(
            self.session.organizer.task(id).unwrap().due.as_deref(),
            Some("2026-10-20")
        );
        assert_eq!(window.focused(cx), focus_before_date);
        let list = self.session.create_task_list("详情移动 smoke".into());
        self.toggle_task_detail_section(TaskDetailSection::List, window, cx);
        let frame = self.render_task_detail(false, cx).unwrap();
        drop(frame);
        self.task_detail_choice = self
            .session
            .organizer
            .lists
            .iter()
            .position(|item| item.id == list)
            .unwrap()
            + 1;
        self.task_detail_key_down(&enter, window, cx);
        assert_eq!(self.session.organizer.task(id).unwrap().list, list);
        self.task_details_open = Some(id);
        let frame = self.render_task_detail(false, cx).unwrap();
        drop(frame);
        let focus_before_escape = window.focused(cx);
        self.toggle_task_detail_section(TaskDetailSection::Priority, window, cx);
        self.task_detail_key_down(&escape, window, cx);
        assert!(self.task_detail_section.is_none());
        assert_eq!(window.focused(cx), focus_before_escape);
        self.close_selected_task(window, cx);
        assert!(self.selected_task.is_none());
        self.organizer_open = previous.0;
        self.organizer_tasks = previous.1;
        if let Some(note) = previous.2 {
            self.select_note(note, window, cx);
        } else if let Some(task) = previous.3 {
            self.select_task(task, window, cx);
        }
        self.selected_task_list = previous.4;
        self.selected_task_smart_view = previous.5;
        if let Some(focus) = previous.6 {
            window.focus(&focus);
        }
        eprintln!(
            "SMOKE: task detail attribute selection, draft preservation, date validation, list move and Escape focus passed"
        );
    }
}

fn metadata_row(label: &'static str, value: String, muted: Hsla, text: Hsla) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(12.))
        .text_size(px(11.))
        .child(
            div()
                .w(px(52.))
                .flex_shrink_0()
                .text_color(muted)
                .child(label),
        )
        .child(div().flex_1().min_w_0().text_color(text).child(value))
}

impl Workspace {
    pub(super) fn toggle_task_detail_section(
        &mut self,
        section: TaskDetailSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.selected_task else { return };
        if self.task_detail_section == Some((id, section)) {
            self.close_task_detail_section(window, cx);
            return;
        }
        self.commit_subtask_draft(window, cx);
        self.commit_note_draft(window, cx);
        if self.task_detail_section.is_none() {
            self.task_detail_focus_return = window.focused(cx);
        }
        self.task_detail_section = Some((id, section));
        self.task_detail_choice = self
            .session
            .organizer
            .task(id)
            .map_or(0, |task| match section {
                TaskDetailSection::Priority => Priority::ALL
                    .iter()
                    .position(|value| *value == task.priority)
                    .unwrap_or(0),
                TaskDetailSection::Repeat => Repeat::ALL
                    .iter()
                    .position(|value| *value == task.repeat)
                    .unwrap_or(0),
                TaskDetailSection::List => std::iter::once(ListId::INBOX)
                    .chain(self.session.organizer.lists.iter().map(|list| list.id))
                    .position(|value| value == task.list)
                    .unwrap_or(0),
                TaskDetailSection::Date => 0,
            });
        if section == TaskDetailSection::Date {
            window.focus(&self.task_due_input.read(cx).focus);
        } else {
            window.focus(&self.task_detail_focus);
        }
        cx.notify();
    }

    pub(super) fn close_task_detail_section(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_note_draft(window, cx);
        self.task_detail_section = None;
        if let Some(focus) = self.task_detail_focus_return.take() {
            window.focus(&focus);
        }
        cx.notify();
    }

    fn set_detail_priority(
        &mut self,
        value: Priority,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.selected_task else { return };
        self.commit_note_draft(window, cx);
        if self.session.set_task_priority(id, value) {
            self.schedule_save(true, window, cx);
        }
        self.close_task_detail_section(window, cx);
    }

    fn set_detail_repeat(&mut self, value: Repeat, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_task else { return };
        self.commit_note_draft(window, cx);
        if self.session.set_task_repeat(id, value) {
            self.schedule_save(true, window, cx);
        }
        self.close_task_detail_section(window, cx);
    }

    fn set_detail_date(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        self.task_due_draft = value.clone();
        self.task_due_input.update(cx, |input, cx| {
            input.reset(value.clone(), Vec::new(), value.len(), cx)
        });
        self.commit_note_draft(window, cx);
        if !self.task_due_error {
            self.close_task_detail_section(window, cx);
        }
    }

    fn move_detail_task(&mut self, list: ListId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_task else { return };
        self.move_task_to_list(id, list, window, cx);
        self.close_task_detail_section(window, cx);
    }

    pub(super) fn task_detail_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((id, section)) = self.task_detail_section else {
            return;
        };
        if self.selected_task != Some(id) {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close_task_detail_section(window, cx);
            cx.stop_propagation();
            return;
        }
        let count = match section {
            TaskDetailSection::Priority => Priority::ALL.len(),
            TaskDetailSection::Repeat => Repeat::ALL.len(),
            TaskDetailSection::List => self.session.organizer.lists.len() + 1,
            TaskDetailSection::Date => return,
        };
        match key {
            "up" | "left" => {
                self.task_detail_choice = (self.task_detail_choice + count - 1) % count
            }
            "down" | "right" => self.task_detail_choice = (self.task_detail_choice + 1) % count,
            "enter" | "space" => match section {
                TaskDetailSection::Priority => self.set_detail_priority(
                    Priority::ALL[self.task_detail_choice % count],
                    window,
                    cx,
                ),
                TaskDetailSection::Repeat => {
                    self.set_detail_repeat(Repeat::ALL[self.task_detail_choice % count], window, cx)
                }
                TaskDetailSection::List => {
                    let list = std::iter::once(ListId::INBOX)
                        .chain(self.session.organizer.lists.iter().map(|list| list.id))
                        .nth(self.task_detail_choice % count)
                        .unwrap_or(ListId::INBOX);
                    self.move_detail_task(list, window, cx);
                }
                TaskDetailSection::Date => {}
            },
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn render_task_detail(
        &self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let task = self
            .selected_task
            .and_then(|id| self.session.organizer.task(id))
            .cloned()?;
        let id = task.id;
        let p = self.palette.clone();
        let text = Palette::color(&p.fg);
        let muted = Palette::color(&p.muted);
        let accent = Palette::color(&p.accent);
        let hover = Palette::color(&p.elevated2);
        let border = Palette::color(&p.border).opacity(0.5);
        let section = self
            .task_detail_section
            .filter(|(task, _)| *task == id)
            .map(|(_, section)| section);
        let list_name = self
            .session
            .organizer
            .list(task.list)
            .map(|list| list.name.clone())
            .unwrap_or_else(|| "收集箱".into());
        let priority_color = match task.priority {
            Priority::High => Hsla::from(rgb(0xe05b61)),
            Priority::Medium => Hsla::from(rgb(0xdba544)),
            Priority::Low => Hsla::from(rgb(0x5f91dc)),
            Priority::None => muted,
        };
        let date_label = if self.task_due_error {
            "日期格式有误".to_owned()
        } else {
            task.due.clone().unwrap_or_else(|| "设置日期".into())
        };
        let details_open = self.task_details_open == Some(id);
        let subtasks = task.subtasks.clone();
        let total = subtasks.len();
        let completed = subtasks.iter().filter(|subtask| subtask.done).count();
        let property_panel = section.map(|section| {
            div()
                .id("task-detail-property-panel")
                .w_full()
                .flex_shrink_0()
                .rounded(px(7.))
                .p(px(10.))
                .bg(hover.opacity(0.55))
                .when(section == TaskDetailSection::Date, |panel| {
                    panel
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .h(px(32.))
                                        .border_b_1()
                                        .border_color(if self.task_due_error {
                                            Hsla::from(rgb(0xe05b61))
                                        } else {
                                            border
                                        })
                                        .child(self.task_due_input.clone()),
                                )
                                .child(
                                    div()
                                        .id("task-detail-date-apply")
                                        .px(px(8.))
                                        .py(px(5.))
                                        .rounded(px(4.))
                                        .text_size(px(12.))
                                        .text_color(accent)
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.set_detail_date(
                                                this.task_due_draft.clone(),
                                                window,
                                                cx,
                                            )
                                        }))
                                        .child("确定"),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(8.))
                                .flex()
                                .gap(px(8.))
                                .text_size(px(11.))
                                .text_color(muted)
                                .child(
                                    div()
                                        .id("task-detail-date-today")
                                        .cursor_pointer()
                                        .hover(|button| button.text_color(accent))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.set_detail_date(
                                                quire_core::core::date::today_iso(),
                                                window,
                                                cx,
                                            )
                                        }))
                                        .child("今天"),
                                )
                                .child(
                                    div()
                                        .id("task-detail-date-clear")
                                        .cursor_pointer()
                                        .hover(|button| button.text_color(accent))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.set_detail_date(String::new(), window, cx)
                                        }))
                                        .child("清除日期"),
                                ),
                        )
                        .when(self.task_due_error, |panel| {
                            panel.child(
                                div()
                                    .mt(px(6.))
                                    .text_size(px(11.))
                                    .text_color(rgb(0xe05b61))
                                    .child("请使用 YYYY-MM-DD，例如 2026-10-16"),
                            )
                        })
                })
                .when(section == TaskDetailSection::Priority, |panel| {
                    panel.flex().flex_wrap().gap(px(5.)).children(
                        Priority::ALL.into_iter().enumerate().map(|(index, value)| {
                            div()
                                .id(SharedString::from(format!("task-detail-priority-{index}")))
                                .px(px(10.))
                                .py(px(7.))
                                .rounded(px(5.))
                                .text_size(px(12.))
                                .cursor_pointer()
                                .text_color(if value == task.priority {
                                    accent
                                } else {
                                    muted
                                })
                                .when(index == self.task_detail_choice, |button| {
                                    button.bg(accent.opacity(0.12))
                                })
                                .hover(|button| button.bg(accent.opacity(0.12)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_detail_priority(value, window, cx)
                                }))
                                .child(if value == Priority::None {
                                    "无优先级".to_owned()
                                } else {
                                    format!("{}优先级", priority_label(value))
                                })
                        }),
                    )
                })
                .when(section == TaskDetailSection::Repeat, |panel| {
                    panel.flex().flex_wrap().gap(px(5.)).children(
                        Repeat::ALL.into_iter().enumerate().map(|(index, value)| {
                            div()
                                .id(SharedString::from(format!("task-detail-repeat-{index}")))
                                .px(px(10.))
                                .py(px(7.))
                                .rounded(px(5.))
                                .text_size(px(12.))
                                .cursor_pointer()
                                .text_color(if value == task.repeat { accent } else { muted })
                                .when(index == self.task_detail_choice, |button| {
                                    button.bg(accent.opacity(0.12))
                                })
                                .hover(|button| button.bg(accent.opacity(0.12)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_detail_repeat(value, window, cx)
                                }))
                                .child(repeat_label(value))
                        }),
                    )
                })
                .when(section == TaskDetailSection::List, |panel| {
                    panel
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .max_h(px(170.))
                        .overflow_y_scroll()
                        .children(
                            std::iter::once((ListId::INBOX, "收集箱".to_owned()))
                                .chain(
                                    self.session
                                        .organizer
                                        .lists
                                        .iter()
                                        .map(|list| (list.id, list.name.clone())),
                                )
                                .enumerate()
                                .map(|(index, (list, name))| {
                                    div()
                                        .id(SharedString::from(format!(
                                            "task-detail-move-{}",
                                            list.0
                                        )))
                                        .px(px(10.))
                                        .py(px(7.))
                                        .rounded(px(5.))
                                        .text_size(px(12.))
                                        .cursor_pointer()
                                        .text_color(if list == task.list { accent } else { muted })
                                        .when(index == self.task_detail_choice, |button| {
                                            button.bg(accent.opacity(0.12))
                                        })
                                        .hover(|button| button.bg(accent.opacity(0.12)))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.move_detail_task(list, window, cx)
                                        }))
                                        .child(format!(
                                            "{}{}",
                                            name,
                                            if list == task.list { "  ✓" } else { "" }
                                        ))
                                }),
                        )
                })
        });
        Some(div().id("task-detail-inner").size_full().min_w_0().min_h_0().flex().flex_col().overflow_hidden()
            .track_focus(&self.task_detail_focus).on_key_down(cx.listener(Self::task_detail_key_down))
            .child(div().flex().items_center().justify_between().flex_shrink_0().h(px(30.))
                .child(div().text_size(px(11.)).text_color(muted).child(if task.done {"已完成"}else{"任务"}))
                .child(div().flex().items_center().gap(px(6.))
                    .child(div().id("organizer-task-detail-menu").w(px(28.)).h(px(28.)).rounded(px(5.)).flex().items_center().justify_center().cursor_pointer().text_color(muted).hover(|button|button.bg(hover))
                        .on_click(cx.listener(move|this,event:&ClickEvent,window,cx|this.open_organizer_context_from_click(1,id.0,event,window,cx))).child("⋯"))
                    .child(div().id("organizer-task-detail-close").px(px(8.)).py(px(5.)).rounded(px(5.)).cursor_pointer().text_size(px(12.)).text_color(muted).hover(|button|button.bg(hover))
                        .on_click(cx.listener(|this,_,window,cx|this.close_selected_task(window,cx))).child(if compact {"‹ 返回"}else{"×"}))))
            .child(div().mt(px(12.)).mb(px(16.)).flex().items_start().gap(px(12.)).flex_shrink_0()
                .child(div().id("organizer-task-toggle-done").mt(px(9.)).w(px(22.)).h(px(22.)).flex_shrink_0().rounded(px(11.)).border_1().border_color(if task.done {accent}else{priority_color}).bg(if task.done {accent}else{Hsla::transparent_black()})
                    .flex().items_center().justify_center().text_size(px(13.)).text_color(rgb(0xffffff)).cursor_pointer()
                    .when(!self.organizer_trash,|button|button.on_click(cx.listener(move|this,_,window,cx|this.toggle_task_done(id,window,cx)))).child(if task.done {"✓"}else{""}))
                .child(div().id("organizer-task-detail-title").flex_1().min_w_0().child(self.note_title_input.clone())))
            .child(div().flex().flex_wrap().gap(px(6.)).flex_shrink_0().mb(px(12.)).text_size(px(12.)).whitespace_nowrap()
                .child(div().id("organizer-task-date-toggle").px(px(8.)).py(px(6.)).rounded(px(5.)).text_color(if self.task_due_error {Hsla::from(rgb(0xe05b61))}else{muted}).cursor_pointer().hover(|button|button.bg(hover))
                    .when(section == Some(TaskDetailSection::Date),|button|button.bg(hover))
                    .on_click(cx.listener(|this,_,window,cx|this.toggle_task_detail_section(TaskDetailSection::Date,window,cx))).child(format!("◷ {date_label}")))
                .child(div().id("organizer-task-priority-cycle").px(px(8.)).py(px(6.)).rounded(px(5.)).text_color(priority_color).cursor_pointer().hover(|button|button.bg(hover))
                    .when(section == Some(TaskDetailSection::Priority),|button|button.bg(hover))
                    .on_click(cx.listener(|this,_,window,cx|this.toggle_task_detail_section(TaskDetailSection::Priority,window,cx))).child(if task.priority == Priority::None {"⚑ 优先级".to_owned()}else{format!("⚑ {}优先级",priority_label(task.priority))}))
                .child(div().id("organizer-task-repeat-cycle").px(px(8.)).py(px(6.)).rounded(px(5.)).text_color(if task.repeat == Repeat::None {muted}else{accent}).cursor_pointer().hover(|button|button.bg(hover))
                    .when(section == Some(TaskDetailSection::Repeat),|button|button.bg(hover))
                    .on_click(cx.listener(|this,_,window,cx|this.toggle_task_detail_section(TaskDetailSection::Repeat,window,cx))).child(format!("⟳ {}",repeat_label(task.repeat)))))
            .when_some(property_panel,|pane,panel|pane.child(panel))
            .child(div().id("task-detail-content").flex_1().min_h_0().min_w_0().overflow_y_scroll().pt(px(16.)).pb(px(24.)).flex().flex_col().gap(px(24.))
                .child(div().flex().flex_col().gap(px(10.)).flex_shrink_0()
                    .child(div().text_size(px(11.)).text_color(muted).child("描述"))
                    .child(div().id("organizer-note-body").min_h(px(96.)).child(self.note_body_input.clone())))
                .child(div().flex().flex_col().gap(px(8.)).flex_shrink_0()
                    .child(div().text_size(px(11.)).text_color(muted).child(format!("子任务  {completed}/{total}")))
                    .children(subtasks.into_iter().map(|subtask| {
                        let subtask_id = subtask.id;
                        let done = subtask.done;
                        let title = subtask.title;
                        let edit_title = title.clone();
                        div().flex().items_center().gap(px(10.)).min_h(px(32.))
                            .child(div().id(SharedString::from(format!("organizer-subtask-done-{id:?}-{subtask_id}"))).w(px(16.)).h(px(16.)).flex_shrink_0().rounded(px(8.)).border_1().border_color(if done {accent}else{muted.opacity(0.6)}).text_color(accent)
                                .flex().items_center().justify_center().text_size(px(10.)).cursor_pointer()
                                .when(!self.organizer_trash,|button|button.on_click(cx.listener(move|this,_,window,cx|this.toggle_subtask_done(id,subtask_id,done,window,cx)))).child(if done {"✓"}else{""}))
                            .when(self.editing_subtask == Some((id,subtask_id)),|row|row.child(div().flex_1().min_w_0().min_h(px(30.)).child(self.subtask_title_input.clone())))
                            .when(self.editing_subtask != Some((id,subtask_id)),|row|row.child(div().id(SharedString::from(format!("organizer-subtask-title-{id:?}-{subtask_id}"))).flex_1().min_w_0().text_size(px(13.)).text_color(if done {muted}else{text}).cursor_text()
                                .on_click(cx.listener(move|this,_,window,cx|this.start_edit_subtask(id,subtask_id,edit_title.clone(),window,cx))).child(title)))
                            .when(!self.organizer_trash,|row|row.child(div().id(SharedString::from(format!("organizer-subtask-delete-{id:?}-{subtask_id}"))).w(px(24.)).text_color(muted.opacity(0.45)).hover(|button|button.text_color(rgb(0xe5534b))).cursor_pointer()
                                .on_click(cx.listener(move|this,_,window,cx|this.delete_subtask(id,subtask_id,window,cx))).child("×")))
                    }))
                    .when(!self.organizer_trash && self.editing_subtask.is_none(),|list|list.child(div().flex().items_center().gap(px(10.)).min_h(px(34.))
                        .child(div().w(px(16.)).text_color(muted).child("＋"))
                        .child(div().flex_1().min_w_0().min_h(px(30.)).child(self.subtask_title_input.clone()))
                        .child(div().id("organizer-subtask-add").px(px(6.)).py(px(4.)).rounded(px(4.)).text_size(px(11.)).text_color(muted).hover(|button|button.text_color(accent)).cursor_pointer()
                            .on_click(cx.listener(|this,_,window,cx|this.add_subtask_from_draft(window,cx))).child("添加")))))
                .child(div().flex().flex_col().gap(px(14.)).flex_shrink_0()
                    .child(div().id("task-detail-list-toggle").flex().items_center().gap(px(10.)).text_size(px(12.)).cursor_pointer().text_color(muted).hover(|button|button.text_color(accent))
                        .child(div().w(px(32.)).child("清单")).child(div().flex_1().min_w_0().child(list_name)).child("⌄")
                        .on_click(cx.listener(|this,_,window,cx|this.toggle_task_detail_section(TaskDetailSection::List,window,cx))))
                    .child(div().flex().items_center().gap(px(10.)).text_size(px(12.)).text_color(muted)
                        .child(div().w(px(32.)).child("标签"))
                        .child(div().id("organizer-tags-input").flex_1().min_w_0().min_h(px(30.)).child(self.organizer_tags_input.clone()))))
                .when(details_open,|content|content.child(div().id("task-detail-metadata").flex().flex_col().gap(px(10.)).pt(px(12.)).border_t_1().border_color(border)
                    .child(metadata_row("编号",id.0.to_string(),muted,text))
                    .child(metadata_row("唯一 ID",if task.uuid.is_empty(){format!("local:{}",id.0)}else{task.uuid.clone()},muted,text))
                    .child(metadata_row("创建",timestamp_label(task.created),muted,text))
                    .child(metadata_row("修改",timestamp_label(task.edited),muted,text)))))
            .child(div().pt(px(12.)).border_t_1().border_color(border).flex_shrink_0().flex().flex_wrap().items_center().justify_between().gap(px(8.)).text_size(px(11.)).text_color(muted)
                .child(div().id("organizer-task-detail-copy").cursor_pointer().hover(|button|button.text_color(accent))
                    .on_click(cx.listener(move|this,_,window,cx|this.run_organizer_context_action(1,id.0,1,window,cx))).child("复制内容和 ID"))
                .child(div().id("task-detail-metadata-toggle").cursor_pointer().hover(|button|button.text_color(accent))
                    .on_click(cx.listener(move|this,_,_,cx|{this.task_details_open=if this.task_details_open==Some(id){None}else{Some(id)};cx.notify();}))
                    .child(if details_open {"收起信息 ⌃"}else{"详细信息 ⌄"})))
            .into_any_element())
    }
}
