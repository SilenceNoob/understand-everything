use makepad_widgets::*;


use crate::ai::{self};
use crate::App;

impl MatchEvent for App {
    fn handle_http_response(&mut self, cx: &mut Cx, request_id: LiveId, response: &HttpResponse) {
        if request_id == self.test_id && self.testing {
            self.testing = false;
            let status = response.status_code;
            let msg = match status {
                200 => "连接成功".to_string(),
                401 => "认证失败：API Key 无效".to_string(),
                _ => {
                    let detail = response
                        .get_string_body()
                        .and_then(|b| ai::body_error_message(&b))
                        .unwrap_or_default();
                    format!("连接失败 ({})：{}", status, detail)
                }
            };
            self.popup_child(
                live_id!(setting_popup),
                &[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(status),
                ],
            )
            .set_text(cx, &msg);
            return;
        }
        if self.gen.is_gen_request(request_id) {
            self.handle_gen_response(cx, request_id, response);
            return;
        }
        if self.gen.is_subcard_request(request_id) {
            self.handle_subcard_response(cx, request_id, response);
            return;
        }
        if self.gen.is_create_request(request_id) {
            self.handle_create_response(cx, request_id, response);
            return;
        }
        if self.gen.is_reorder_request(request_id) {
            self.handle_reorder_response(cx, request_id, response);
            return;
        }
        if request_id == self.diag.diag_id && self.diag.diag_id != LiveId::empty() {
            self.handle_diag_response(cx, response);
            return;
        }
        if request_id == self.quiz.quiz_id && self.quiz.quiz_id != LiveId::empty() {
            self.handle_quiz_response(cx, response);
            return;
        }
        if request_id == self.quiz.grade_id && self.quiz.grade_id != LiveId::empty() {
            self.handle_grade_response(cx, response);
            return;
        }
        if request_id == self.diag.map_name_id && self.diag.map_name_id != LiveId::empty() {
            self.handle_map_name_response(cx, response);
            return;
        }
    }

    fn handle_http_request_error(&mut self, cx: &mut Cx, request_id: LiveId, err: &HttpError) {
        // A streaming structured request whose SSE stream already saw the
        // protocol terminator (`[DONE]`) is complete: makepad's OpenSSL read
        // loop reports a peer's clean close_notify as "unknown OpenSSL
        // error" AFTER the full body has arrived — salvage the reply
        // instead of failing the request. A stream without [DONE] is a real
        // truncation and falls through to the error arms below.
        if let Some(mut stream) = self.take_struct_stream(request_id) {
            stream.finish();
            if let Some(response) = stream.saw_done().then(|| stream.into_response(200)) {
                self.handle_http_response(cx, request_id, &response);
                return;
            }
        }
        if request_id == self.test_id && self.testing {
            self.testing = false;
            self.popup_child(
                live_id!(setting_popup),
                &[
                    live_id!(content),
                    live_id!(panel),
                    live_id!(settings_form),
                    live_id!(status),
                ],
            )
            .set_text(cx, &format!("连接失败：{}", err.message));
            return;
        }
        if self.gen.is_gen_request(request_id) {
            self.gen.gen_request_failed(
                cx,
                request_id,
                format!("生成请求失败：{}", err.message),
                &mut self.toast_until,
            );
            return;
        }
        if self.gen.is_subcard_request(request_id) {
            self.gen.subcard_request_failed(
                cx,
                request_id,
                format!("生成子卡片失败：{}", err.message),
                &mut self.toast_until,
            );
            return;
        }
        if self.gen.is_create_request(request_id) {
            self.gen.create_request_failed(
                cx,
                request_id,
                format!("创建卡片失败：{}", err.message),
                &mut self.toast_until,
            );
            return;
        }
        if self.gen.is_reorder_request(request_id) {
            self.gen.reorder_request_failed(
                cx,
                request_id,
                format!("序号估计失败：{}", err.message),
                &mut self.toast_until,
            );
            return;
        }
        if request_id == self.route.route_id && self.route.route_id != LiveId::empty() {
            self.route.route_buf.clear();
            self.abort_route(cx, format!("路线规划请求失败：{}", err.message));
            return;
        }
        if request_id == self.diag.diag_id && self.diag.diag_id != LiveId::empty() {
            self.diag.diag_id = LiveId::empty();
            self.set_diag_status(cx, &format!("出题请求失败：{}", err.message));
            return;
        }
        if request_id == self.quiz.quiz_id && self.quiz.quiz_id != LiveId::empty() {
            self.quiz.quiz_id = LiveId::empty();
            self.open_quiz_popup(cx);
            self.quiz_panel().show_error(cx, &format!("出题请求失败：{}", err.message));
            return;
        }
        if request_id == self.quiz.grade_id && self.quiz.grade_id != LiveId::empty() {
            self.quiz.grade_id = LiveId::empty();
            self.open_quiz_popup(cx);
            self.quiz_panel().grade_failed(cx, &format!("评分请求失败：{}", err.message));
            return;
        }
        if request_id == self.diag.map_name_id && self.diag.map_name_id != LiveId::empty() {
            self.handle_map_name_error(cx);
            return;
        }
        if request_id == self.chat.chat_id && self.chat.chat_pending {
            self.chat.chat_pending = false;
            self.push_chat_msg(cx, "assistant", &format!("请求失败：{}", err.message));
        }
    }

    /// A chunk of the streaming reply; feed it to the SSE parser and refresh
    /// the "思考中…" bubble with the accumulated text.
    fn handle_http_stream(&mut self, cx: &mut Cx, request_id: LiveId, data: &HttpResponse) {
        if request_id == self.route.route_id && self.route.route_id != LiveId::empty() {
            if let Some(bytes) = data.body() {
                let (content, _thinking) = self.route.route_parser.feed(bytes);
                for delta in content {
                    self.route.route_buf.push_str(&delta);
                }
            }
            return;
        }
        // Streaming structured requests (quiz/grade/diag/gen): accumulate
        // the SSE deltas; the finalize happens on completion (or on a
        // post-[DONE] transport error) in the handlers below.
        if self.struct_stream_mut(request_id).is_some() {
            let mut received = 0;
            if let Some(bytes) = data.body() {
                let stream = self.struct_stream_mut(request_id).unwrap();
                stream.feed(bytes);
                received = stream.content_chars();
            }
            if request_id == self.quiz.quiz_id
                && self.quiz.quiz_id != LiveId::empty()
                && received > 0
            {
                let title = self.quiz.quiz_title();
                self.quiz_panel().update_loading_progress(
                    cx,
                    &format!("{title}: 出题中… 已生成 {received} 字"),
                );
            }
            return;
        }
        if request_id != self.chat.chat_id || !self.chat.chat_pending {
            return;
        }
        if let Some(bytes) = data.body() {
            let (content, thinking) = self.chat.chat_parser.feed(bytes);
            for delta in content {
                self.chat.chat_buf.push_str(&delta);
            }
            for delta in thinking {
                self.chat.chat_think.push_str(&delta);
            }
            self.render_msgs(cx);
        }
    }

    fn handle_http_stream_complete(&mut self, cx: &mut Cx, request_id: LiveId, data: &HttpResponse) {
        if request_id == self.route.route_id && self.route.route_id != LiveId::empty() {
            self.route.route_id = LiveId::empty();
            // macOS 的 makepad 流式后端把 status_code 硬编码为 0，成功流按
            // "status 0 + [DONE]" 判定（同 chat 的处理）。
            let ok = data.status_code == 200
                || (data.status_code == 0 && self.route.route_parser.raw().contains("[DONE]"));
            let buf = std::mem::take(&mut self.route.route_buf);
            if ok {
                self.apply_route_plan(cx, buf);
            } else {
                // Non-200 stream: the body was raw JSON (not SSE), recovered
                // from the parser's raw buffer.
                let raw = self.route.route_parser.raw();
                let detail = ai::body_error_message(&raw)
                    .unwrap_or_else(|| raw.chars().take(200).collect());
                self.abort_route(cx, format!("路线规划失败 ({}): {}", data.status_code, detail));
            }
            return;
        }
        // Streaming structured requests (quiz/grade/diag/gen): synthesize
        // the accumulated SSE back into a single-shot HttpResponse and reuse
        // the regular response dispatch, so all existing handlers (JSON
        // parsing, 400/422 fallback retries, repair retries) keep working.
        if let Some(stream) = self.take_struct_stream(request_id) {
            let response = stream.into_response(data.status_code);
            self.handle_http_response(cx, request_id, &response);
            return;
        }
        if request_id != self.chat.chat_id || !self.chat.chat_pending {
            return;
        }
        self.chat.chat_pending = false;
        // macOS 的 makepad 流式后端在连接正常结束时把 status_code 硬编码为
        // 0（从不记录真实 HTTP 状态），所以成功流要按 "status 0 + [DONE]"
        // 判定；Linux/Windows 传真实 200，不受影响。
        let ok = data.status_code == 200
            || (data.status_code == 0 && self.chat.chat_parser.raw().contains("[DONE]"));
        let content = if ok {
            self.chat.chat_buf.clone()
        } else {
            // Non-200 stream: the body was raw JSON (not SSE), recovered from
            // the parser's raw buffer.
            let raw = self.chat.chat_parser.raw();
            let detail = ai::body_error_message(&raw)
                .unwrap_or_else(|| raw.chars().take(200).collect());
            format!("请求失败 ({}): {}", data.status_code, detail)
        };
        if ok {
            let buf = std::mem::take(&mut self.chat.chat_buf);
            let think = std::mem::take(&mut self.chat.chat_think);
            self.push_chat_msg_thinking(cx, &buf, &think);
        } else {
            self.chat.chat_think.clear();
            self.push_chat_msg(cx, "assistant", &content);
        }
    }
}

impl App {
    /// The StructStream accumulating a streaming structured request
    /// (quiz/grade/diag/gen), when that id is in flight.
    fn struct_stream_mut(&mut self, request_id: LiveId) -> Option<&mut ai::StructStream> {
        if request_id == self.quiz.quiz_id && self.quiz.quiz_id != LiveId::empty() {
            Some(&mut self.quiz.quiz_stream)
        } else if request_id == self.quiz.grade_id && self.quiz.grade_id != LiveId::empty() {
            Some(&mut self.quiz.grade_stream)
        } else if request_id == self.diag.diag_id && self.diag.diag_id != LiveId::empty() {
            Some(&mut self.diag.diag_stream)
        } else {
            self.gen.streams.get_mut(&request_id)
        }
    }

    /// Take the finished accumulator for request_id, if any.
    fn take_struct_stream(&mut self, request_id: LiveId) -> Option<ai::StructStream> {
        if request_id == self.quiz.quiz_id && self.quiz.quiz_id != LiveId::empty() {
            Some(std::mem::take(&mut self.quiz.quiz_stream))
        } else if request_id == self.quiz.grade_id && self.quiz.grade_id != LiveId::empty() {
            Some(std::mem::take(&mut self.quiz.grade_stream))
        } else if request_id == self.diag.diag_id && self.diag.diag_id != LiveId::empty() {
            Some(std::mem::take(&mut self.diag.diag_stream))
        } else {
            self.gen.streams.remove(&request_id)
        }
    }
}

