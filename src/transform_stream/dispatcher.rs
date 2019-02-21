use super::transform_controller::*;
use crate::base::{Chunk, Range};
use crate::parser::{
    Lexeme, LexemeSink, NonTagContentLexeme, ParserDirective, ParserOutputSink, TagHint,
    TagHintSink, TagLexeme, TagNameInfo, TagTokenOutline,
};
use crate::token::{Serialize, ToToken, TokenCapturer, TokenCapturerEvent};
use encoding_rs::Encoding;
use std::rc::Rc;

use TagTokenOutline::*;

pub trait OutputSink {
    fn handle_chunk(&mut self, chunk: &[u8]);
}

impl<F: FnMut(&[u8])> OutputSink for F {
    fn handle_chunk(&mut self, chunk: &[u8]) {
        self(chunk);
    }
}

pub struct Dispatcher<C, O>
where
    C: TransformController,
    O: OutputSink,
{
    transform_controller: C,
    output_sink: O,
    last_consumed_lexeme_end: usize,
    token_capturer: TokenCapturer,
    got_capture_flags_from_tag_hint: bool,
    pending_element_modifiers_info_handler: Option<ElementModifiersInfoHandler<C>>,
}

impl<C, O> Dispatcher<C, O>
where
    C: TransformController,
    O: OutputSink,
{
    pub fn new(transform_controller: C, output_sink: O, encoding: &'static Encoding) -> Self {
        let initial_capture_flags = transform_controller
            .document_level_content_settings()
            .into();

        Dispatcher {
            transform_controller,
            output_sink,
            last_consumed_lexeme_end: 0,
            token_capturer: TokenCapturer::new(initial_capture_flags, encoding),
            got_capture_flags_from_tag_hint: false,
            pending_element_modifiers_info_handler: None,
        }
    }

    pub fn flush_remaining_input(&mut self, input: &Chunk<'_>, blocked_byte_count: usize) {
        let output = input.slice(Range {
            start: self.last_consumed_lexeme_end,
            end: input.len() - blocked_byte_count,
        });

        if !output.is_empty() {
            self.output_sink.handle_chunk(&output);
        }

        self.last_consumed_lexeme_end = 0;
    }

    fn try_produce_token_from_lexeme<'i, T>(&mut self, lexeme: &Lexeme<'i, T>)
    where
        Lexeme<'i, T>: ToToken,
    {
        let transform_controller = &mut self.transform_controller;
        let output_sink = &mut self.output_sink;
        let lexeme_range = lexeme.raw_range();
        let last_consumed_lexeme_end = self.last_consumed_lexeme_end;
        let mut lexeme_consumed = false;

        self.token_capturer.feed(lexeme, |event| match event {
            TokenCapturerEvent::LexemeConsumed => {
                let chunk = lexeme.input().slice(Range {
                    start: last_consumed_lexeme_end,
                    end: lexeme_range.start,
                });

                lexeme_consumed = true;
                output_sink.handle_chunk(&chunk);
            }
            TokenCapturerEvent::TokenProduced(mut token) => {
                trace!(@output token);

                transform_controller.handle_token(&mut token);
                token.to_bytes(&mut |c| output_sink.handle_chunk(c));
            }
        });

        if lexeme_consumed {
            self.last_consumed_lexeme_end = lexeme_range.end;
        }
    }

    #[inline]
    fn get_next_parser_directive(&self) -> ParserDirective {
        if self.token_capturer.has_captures() {
            ParserDirective::Lex
        } else {
            ParserDirective::ScanForTags
        }
    }

    fn adjust_capture_flags_for_tag_lexeme(&mut self, lexeme: &TagLexeme<'_>) {
        let input = lexeme.input();

        macro_rules! get_flags_from_handler {
            ($handler:expr, $attributes:expr, $self_closing:expr) => {
                $handler(
                    &mut self.transform_controller,
                    ElementModifiersInfo::new(input, Rc::clone($attributes), $self_closing),
                )
                .into()
            };
        }

        let capture_flags = match self.pending_element_modifiers_info_handler.take() {
            // NOTE: tag hint was produced for the tag, but
            // attributes and self closing flag were requested.
            Some(mut handler) => match lexeme.token_outline() {
                StartTag {
                    attributes,
                    self_closing,
                    ..
                } => get_flags_from_handler!(handler, attributes, *self_closing),
                _ => unreachable!("Tag should be a start tag at this point"),
            },

            // NOTE: tag hint hasn't been produced for the tag, because
            // parser is not in the tag scan mode.
            None => match lexeme.token_outline() {
                StartTag {
                    name,
                    name_hash,
                    attributes,
                    self_closing,
                } => {
                    let name_info = TagNameInfo::new(input, *name, *name_hash);

                    match self.transform_controller.handle_element_start(name_info) {
                        ElementStartResponse::ContentSettings(settings) => settings.into(),
                        ElementStartResponse::RequestElementModifiersInfo(mut handler) => {
                            get_flags_from_handler!(handler, attributes, *self_closing)
                        }
                    }
                }

                EndTag { name, name_hash } => {
                    let name_info = TagNameInfo::new(input, *name, *name_hash);

                    self.transform_controller
                        .handle_element_end(name_info)
                        .into()
                }
            },
        };

        self.token_capturer.set_capture_flags(capture_flags);
    }
}

impl<C, O> LexemeSink for Dispatcher<C, O>
where
    C: TransformController,
    O: OutputSink,
{
    fn handle_tag(&mut self, lexeme: &TagLexeme<'_>) -> ParserDirective {
        if self.got_capture_flags_from_tag_hint {
            self.got_capture_flags_from_tag_hint = false;
        } else {
            self.adjust_capture_flags_for_tag_lexeme(lexeme);
        }

        self.try_produce_token_from_lexeme(lexeme);

        // NOTE: we capture tag tokens only for the current lexeme and
        // only if it has been requested in content setting. So, once we've
        // handled lexeme for the tag we should disable tag capturing.
        self.token_capturer.stop_capturing_tags();

        self.get_next_parser_directive()
    }

    #[inline]
    fn handle_non_tag_content(&mut self, lexeme: &NonTagContentLexeme<'_>) {
        self.try_produce_token_from_lexeme(lexeme);
    }
}

impl<C, O> TagHintSink for Dispatcher<C, O>
where
    C: TransformController,
    O: OutputSink,
{
    fn handle_tag_hint(&mut self, tag_hint: &TagHint<'_>) -> ParserDirective {
        let capture_flags = match *tag_hint {
            TagHint::StartTag(name_info) => {
                match self.transform_controller.handle_element_start(name_info) {
                    ElementStartResponse::ContentSettings(settings) => settings.into(),
                    ElementStartResponse::RequestElementModifiersInfo(handler) => {
                        self.pending_element_modifiers_info_handler = Some(handler);

                        return ParserDirective::Lex;
                    }
                }
            }
            TagHint::EndTag(name_info) => self
                .transform_controller
                .handle_element_end(name_info)
                .into(),
        };

        self.token_capturer.set_capture_flags(capture_flags);
        self.got_capture_flags_from_tag_hint = true;
        self.get_next_parser_directive()
    }
}

impl<C, O> ParserOutputSink for Dispatcher<C, O>
where
    C: TransformController,
    O: OutputSink,
{
}