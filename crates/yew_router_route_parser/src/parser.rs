//! Parser that consumes a string and produces the first representation of the matcher.
use crate::{
    core::{
        capture, capture_single, exact, get_and, get_end, get_hash, get_question, get_slash, query,
    },
    error::{get_reason, ParseError, ParserErrorReason, PrettyParseError},
    FieldType,
};
use nom::{branch::alt, IResult};

/// Tokens generated from parsing a route matcher string.
/// They will be optimized to another token type that is used to match URLs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouteParserToken<'a> {
    /// Match /
    Separator,
    /// Match a specific string.
    Exact(&'a str),
    /// Match {_}. See `RefCaptureVariant` for more.
    Capture(RefCaptureVariant<'a>),
    /// Match ?
    QueryBegin,
    /// Match &
    QuerySeparator,
    /// Match x=y
    Query {
        /// Identifier
        ident: &'a str,
        /// Capture or match
        capture_or_exact: CaptureOrExact<'a>,
    },
    /// Match \#
    FragmentBegin,
    /// Match !
    End,
}

/// Token representing various types of captures.
///
/// It can capture and discard for unnamed variants, or capture and store in the `Matches` for the
/// named variants.
///
/// Its name stems from the fact that it does not have ownership over all its values.
/// It gets converted to CaptureVariant, a nearly identical enum that has owned Strings instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RefCaptureVariant<'a> {
    /// {}
    Unnamed,
    /// {*}
    ManyUnnamed,
    /// {5}
    NumberedUnnamed {
        /// Number of sections to match.
        sections: usize,
    },
    /// {name} - captures a section and adds it to the map with a given name.
    Named(&'a str),
    /// {*:name} - captures over many sections and adds it to the map with a given name.
    ManyNamed(&'a str),
    /// {2:name} - captures a fixed number of sections with a given name.
    NumberedNamed {
        /// Number of sections to match.
        sections: usize,
        /// The key to be entered in the `Matches` map.
        name: &'a str,
    },
}

/// Either a Capture, or an Exact match
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaptureOrExact<'a> {
    /// Match a specific string.
    Exact(&'a str),
    /// Match a capture variant.
    Capture(RefCaptureVariant<'a>),
}

/// Represents the states the parser can be in.
#[derive(Clone, PartialEq)]
enum ParserState<'a> {
    None,
    Path { prev_token: RouteParserToken<'a> },
    FirstQuery { prev_token: RouteParserToken<'a> },
    NthQuery { prev_token: RouteParserToken<'a> },
    Fragment { prev_token: RouteParserToken<'a> },
    End,
}
impl<'a> ParserState<'a> {
    /// Given a new route parser token, transition to a new state.
    ///
    /// This will set the prev token to a token able to be handled by the new state,
    /// so the new state does not need to handle arbitrary "from" states.
    ///
    /// This function represents the valid state transition graph.
    fn transition(self, token: RouteParserToken<'a>) -> Result<Self, ParserErrorReason> {
        match self {
            ParserState::None => match token {
                RouteParserToken::Separator
                | RouteParserToken::Exact(_)
                | RouteParserToken::Capture(_) => Ok(ParserState::Path { prev_token: token }),
                RouteParserToken::QueryBegin => Ok(ParserState::FirstQuery { prev_token: token }),
                RouteParserToken::QuerySeparator // TODO this may be possible in the future.
                | RouteParserToken::Query { .. } => Err(ParserErrorReason::NotAllowedStateTransition),
                RouteParserToken::FragmentBegin => Ok(ParserState::Fragment { prev_token: token }),
                RouteParserToken::End => Ok(ParserState::End)
            },
            ParserState::Path { prev_token } => {
                match prev_token {
                    RouteParserToken::Separator => match token {
                        RouteParserToken::Exact(_) | RouteParserToken::Capture(_) => {
                            Ok(ParserState::Path { prev_token: token })
                        }
                        RouteParserToken::QueryBegin => {
                            Ok(ParserState::FirstQuery { prev_token: token })
                        }
                        RouteParserToken::FragmentBegin => {
                            Ok(ParserState::Fragment { prev_token: token })
                        }
                        RouteParserToken::End => Ok(ParserState::End),
                        _ => Err(ParserErrorReason::NotAllowedStateTransition),
                    },
                    RouteParserToken::Exact(_) => match token {
                        RouteParserToken::Separator | RouteParserToken::Capture(_) => {
                            Ok(ParserState::Path { prev_token: token })
                        }
                        RouteParserToken::QueryBegin => {
                            Ok(ParserState::FirstQuery { prev_token: token })
                        }
                        RouteParserToken::FragmentBegin => {
                            Ok(ParserState::Fragment { prev_token: token })
                        }
                        RouteParserToken::End => Ok(ParserState::End),
                        _ => Err(ParserErrorReason::NotAllowedStateTransition),
                    },
                    RouteParserToken::Capture(_) => match token {
                        RouteParserToken::Separator | RouteParserToken::Exact(_) => {
                            Ok(ParserState::Path { prev_token: token })
                        }
                        RouteParserToken::QueryBegin => {
                            Ok(ParserState::FirstQuery { prev_token: token })
                        }
                        RouteParserToken::FragmentBegin => {
                            Ok(ParserState::Fragment { prev_token: token })
                        }
                        RouteParserToken::End => Ok(ParserState::End),
                        _ => Err(ParserErrorReason::NotAllowedStateTransition),
                    },
                    _ => Err(ParserErrorReason::InvalidState), /* Other previous token types are
                                                                * invalid within a Path state. */
                }
            }
            ParserState::FirstQuery { prev_token } => match prev_token {
                RouteParserToken::QueryBegin => match token {
                    RouteParserToken::Query { .. } => {
                        Ok(ParserState::FirstQuery { prev_token: token })
                    }
                    _ => Err(ParserErrorReason::NotAllowedStateTransition),
                },
                RouteParserToken::Query { .. } => match token {
                    RouteParserToken::QuerySeparator => {
                        Ok(ParserState::NthQuery { prev_token: token })
                    }
                    RouteParserToken::FragmentBegin => {
                        Ok(ParserState::Fragment { prev_token: token })
                    }
                    RouteParserToken::End => Ok(ParserState::End),
                    _ => Err(ParserErrorReason::NotAllowedStateTransition),
                },
                _ => Err(ParserErrorReason::InvalidState),
            },
            ParserState::NthQuery { prev_token } => match prev_token {
                RouteParserToken::QuerySeparator => match token {
                    RouteParserToken::Query { .. } => {
                        Ok(ParserState::NthQuery { prev_token: token })
                    }
                    _ => Err(ParserErrorReason::NotAllowedStateTransition),
                },
                RouteParserToken::Query { .. } => match token {
                    RouteParserToken::QuerySeparator => {
                        Ok(ParserState::NthQuery { prev_token: token })
                    }
                    RouteParserToken::FragmentBegin => {
                        Ok(ParserState::Fragment { prev_token: token })
                    }
                    RouteParserToken::End => Ok(ParserState::End),
                    _ => Err(ParserErrorReason::NotAllowedStateTransition),
                },
                _ => Err(ParserErrorReason::InvalidState),
            },
            ParserState::Fragment { prev_token } => match prev_token {
                RouteParserToken::FragmentBegin
                | RouteParserToken::Exact(_)
                | RouteParserToken::Capture(_) => Ok(ParserState::Fragment { prev_token: token }),
                RouteParserToken::End => Ok(ParserState::End),
                _ => Err(ParserErrorReason::InvalidState),
            },
            ParserState::End => Err(ParserErrorReason::TokensAfterEndToken),
        }
    }
}

/// Parse a matching string into a vector of RouteParserTokens.
///
/// The parsing logic involves using a state machine.
/// After a token is read, this token is fed into the state machine, causing it to transition to a new state or throw an error.
/// Because the tokens that can be parsed in each state are limited, errors are not actually thrown in the state transition,
/// due to the fact that erroneous tokens can't be fed into the transition function.
///
/// This continues until the string is exhausted, or none of the parsers for the current state can parse the current input.
pub fn parse(
    mut i: &str,
    field_type: FieldType,
) -> Result<Vec<RouteParserToken>, PrettyParseError> {
    let input = i;
    let mut tokens: Vec<RouteParserToken> = vec![];
    let mut state = ParserState::None;

    loop {
        let (ii, token) = parse_impl(i, &state, field_type).map_err(|e| match e {
            nom::Err::Error(e) | nom::Err::Failure(e) => PrettyParseError {
                error: e,
                input,
                remaining: i,
            },
            _ => panic!("parser should not be incomplete"),
        })?;
        i = ii;
        state = state.transition(token.clone()).map_err(|reason| {
            let error = ParseError {
                reason: Some(reason),
                expected: vec![],
                offset: 0,
            };
            PrettyParseError {
                error,
                input,
                remaining: i,
            }
        })?;
        tokens.push(token);

        // If there is no more input, break out of the loop
        if i.is_empty() {
            break;
        }
    }
    Ok(tokens)
}

fn parse_impl<'a>(
    i: &'a str,
    state: &ParserState,
    field_type: FieldType,
) -> IResult<&'a str, RouteParserToken<'a>, ParseError> {
    match state {
        ParserState::None => alt((
            get_slash,
            get_question,
            get_hash,
            capture(field_type),
            exact,
            get_end,
        ))(i)
        .map_err(|mut e: nom::Err<ParseError>| {
            // Detect likely failures if the above failed to match.
            let reason: &mut Option<ParserErrorReason> = get_reason(&mut e);
            *reason = get_and(i).map(|_| ParserErrorReason::AndBeforeQuestion) // TODO, technically, a sub-switch may want to start with a &query=something, so enabling this might make sense.
//                    .or_else(|_| bad_capture(i).map(|(_, reason)| reason))
                    .ok()
                    .or(*reason);
            e
        }),
        ParserState::Path { prev_token } => match prev_token {
            RouteParserToken::Separator => {
                alt((exact, capture(field_type), get_question, get_hash, get_end))(i).map_err(
                    |mut e: nom::Err<ParseError>| {
                        // Detect likely failures if the above failed to match.
                        let reason: &mut Option<ParserErrorReason> = get_reason(&mut e);
                        *reason = get_slash(i)
                            .map(|_| ParserErrorReason::DoubleSlash)
                            .or_else(|_| get_and(i).map(|_| ParserErrorReason::AndBeforeQuestion))
//                            .or_else(|_| bad_capture(i).map(|(_, reason)| reason))
                            .ok()
                            .or(*reason);
                        e
                    },
                )
            }
            RouteParserToken::Exact(_) => {
                alt((
                    get_slash,
                    capture(field_type),
                    get_question,
                    get_hash,
                    get_end,
                ))(i)
                .map_err(|mut e: nom::Err<ParseError>| {
                    // Detect likely failures if the above failed to match.
                    let reason: &mut Option<ParserErrorReason> = get_reason(&mut e);
                    *reason = get_and(i)
                            .map(|_| ParserErrorReason::AndBeforeQuestion)
//                            .or_else(|_| bad_capture(i).map(|(_, reason)| reason))
                            .ok()
                            .or(*reason);
                    e
                })
            }
            RouteParserToken::Capture(_) => {
                alt((get_slash, exact, get_question, get_hash, get_end))(i).map_err(
                    |mut e: nom::Err<ParseError>| {
                        // Detect likely failures if the above failed to match.
                        let reason: &mut Option<ParserErrorReason> = get_reason(&mut e);
                        *reason = capture(field_type)(i)
                            .map(|_| ParserErrorReason::AdjacentCaptures)
                            .or_else(|_| get_and(i).map(|_| ParserErrorReason::AndBeforeQuestion))
                            .ok()
                            .or(*reason);
                        e
                    },
                )
            }
            _ => Err(nom::Err::Failure(ParseError {
                reason: Some(ParserErrorReason::InvalidState),
                expected: vec![],
                offset: 0,
            })),
        },
        ParserState::FirstQuery { prev_token } => match prev_token {
            RouteParserToken::QueryBegin => {
                query(field_type)(i).map_err(|mut e: nom::Err<ParseError>| {
                    // Detect likely failures if the above failed to match.
                    let reason: &mut Option<ParserErrorReason> = get_reason(&mut e);
                    *reason = get_question(i)
                        .map(|_| ParserErrorReason::MultipleQuestions)
                        .ok()
                        .or(*reason);
                    e
                })
            }
            RouteParserToken::Query { .. } => {
                alt((get_and, get_hash, get_end))(i).map_err(|mut e: nom::Err<ParseError>| {
                    // Detect likely failures if the above failed to match.
                    let reason: &mut Option<ParserErrorReason> = get_reason(&mut e);
                    *reason = get_question(i)
                        .map(|_| ParserErrorReason::MultipleQuestions)
                        .ok()
                        .or(*reason);
                    e
                })
            }
            _ => Err(nom::Err::Failure(ParseError {
                reason: Some(ParserErrorReason::InvalidState),
                expected: vec![],
                offset: 0,
            })),
        },
        ParserState::NthQuery { prev_token } => match prev_token {
            RouteParserToken::QuerySeparator => {
                query(field_type)(i).map_err(|mut e: nom::Err<ParseError>| {
                    // Detect likely failures if the above failed to match.
                    let reason: &mut Option<ParserErrorReason> = get_reason(&mut e);
                    *reason = get_question(i)
                        .map(|_| ParserErrorReason::MultipleQuestions)
                        .ok()
                        .or(*reason);
                    e
                })
            }
            RouteParserToken::Query { .. } => {
                alt((get_and, get_hash, get_end))(i).map_err(|mut e: nom::Err<ParseError>| {
                    // Detect likely failures if the above failed to match.
                    let reason: &mut Option<ParserErrorReason> = get_reason(&mut e);
                    *reason = get_question(i)
                        .map(|_| ParserErrorReason::MultipleQuestions)
                        .ok()
                        .or(*reason);
                    e
                })
            }
            _ => Err(nom::Err::Failure(ParseError {
                reason: Some(ParserErrorReason::InvalidState),
                expected: vec![],
                offset: 0,
            })),
        },
        ParserState::Fragment { prev_token } => match prev_token {
            RouteParserToken::FragmentBegin => alt((exact, capture_single(field_type), get_end))(i),
            RouteParserToken::Exact(_) => alt((capture_single(field_type), get_end))(i),
            RouteParserToken::Capture(_) => alt((exact, get_end))(i),
            //                .map_err(|mut e: nom::Err<ParseError>| {
            //                    // Detect likely failures if the above failed to match.
            //                    let reason: &mut Option<ParserErrorReason> = get_reason(&mut e);
            //                    *reason = bad_capture(i).map(|(_, reason)| reason).ok()
            //                        .or(*reason);
            //                    e
            //                }),
            _ => Err(nom::Err::Failure(ParseError {
                reason: Some(ParserErrorReason::InvalidState),
                expected: vec![],
                offset: 0,
            })),
        },
        ParserState::End => Err(nom::Err::Failure(ParseError {
            reason: Some(ParserErrorReason::TokensAfterEndToken),
            expected: vec![],
            offset: 0,
        })),
    }
}

#[cfg(test)]
mod test {
    //    use super::*;
    use super::parse as actual_parse;
    use crate::{parser::RouteParserToken, FieldType, PrettyParseError};

    // Call all tests to parse with the Unnamed variant
    fn parse(i: &str) -> Result<Vec<RouteParserToken>, PrettyParseError> {
        actual_parse(i, FieldType::Unnamed)
    }

    mod does_parse {
        use super::*;

        #[test]
        fn slash() {
            parse("/").expect("should parse");
        }

        #[test]
        fn slash_exact() {
            parse("/hello").expect("should parse");
        }

        #[test]
        fn multiple_exact() {
            parse("/lorem/ipsum").expect("should parse");
        }

        #[test]
        fn capture_in_path() {
            parse("/lorem/{ipsum}").expect("should parse");
        }

        #[test]
        fn capture_rest_in_path() {
            parse("/lorem/{*:ipsum}").expect("should parse");
        }

        #[test]
        fn capture_numbered_in_path() {
            parse("/lorem/{5:ipsum}").expect("should parse");
        }

        #[test]
        fn exact_query_after_path() {
            parse("/lorem?ipsum=dolor").expect("should parse");
        }

        #[test]
        fn exact_query() {
            parse("?lorem=ipsum").expect("should parse");
        }

        #[test]
        fn capture_query() {
            parse("?lorem={ipsum}").expect("should parse");
        }

        #[test]
        fn multiple_queries() {
            parse("?lorem=ipsum&dolor=sit").expect("should parse");
        }

        #[test]
        fn query_and_exact_fragment() {
            parse("?lorem=ipsum#dolor").expect("should parse");
        }

        #[test]
        fn query_with_exact_and_capture_fragment() {
            parse("?lorem=ipsum#dolor{sit}").expect("should parse");
        }

        #[test]
        fn query_with_capture_fragment() {
            parse("?lorem=ipsum#{dolor}").expect("should parse");
        }
    }

    mod does_not_parse {
        use super::*;
        use crate::error::{ExpectedToken, ParserErrorReason};

        // TODO, should empty be ok?
        #[test]
        fn empty() {
            parse("").expect_err("Should not parse");
        }

        #[test]
        fn double_slash() {
            let x = parse("//").expect_err("Should not parse");
            assert_eq!(x.error.reason, Some(ParserErrorReason::DoubleSlash))
        }

        #[test]
        fn slash_ampersand() {
            let x = parse("/&lorem=ipsum").expect_err("Should not parse");
            assert_eq!(x.error.reason, Some(ParserErrorReason::AndBeforeQuestion))
        }

        #[test]
        fn non_ident_capture() {
            let x = parse("/{lor#m}").expect_err("Should not parse");
            assert_eq!(x.error.reason, Some(ParserErrorReason::BadRustIdent('#')));
            assert_eq!(
                x.error.expected,
                vec![ExpectedToken::CloseBracket, ExpectedToken::Ident]
            )
        }

        #[test]
        fn leading_ampersand_query() {
            let x = parse("&query=thing").expect_err("Should not parse");
            assert_eq!(x.error.reason, Some(ParserErrorReason::AndBeforeQuestion));
        }

        #[test]
        fn after_end() {
            let x = parse("/lorem/ipsum!/dolor").expect_err("Should not parse");
            assert_eq!(x.error.reason, Some(ParserErrorReason::TokensAfterEndToken));
        }

        #[test]
        fn double_end() {
            let x = parse("/hello!!").expect_err("Should not parse");
            assert_eq!(x.error.reason, Some(ParserErrorReason::TokensAfterEndToken));
        }
    }

    mod correct_parse {
        use super::*;
        use crate::parser::{CaptureOrExact, RefCaptureVariant};

        #[test]
        fn starting_literal() {
            let parsed = parse("lorem").unwrap();
            let expected = vec![RouteParserToken::Exact("lorem")];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn minimal_path() {
            let parsed = parse("/lorem").unwrap();
            let expected = vec![
                RouteParserToken::Separator,
                RouteParserToken::Exact("lorem"),
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn multiple_path() {
            let parsed = parse("/lorem/ipsum/dolor/sit").unwrap();
            let expected = vec![
                RouteParserToken::Separator,
                RouteParserToken::Exact("lorem"),
                RouteParserToken::Separator,
                RouteParserToken::Exact("ipsum"),
                RouteParserToken::Separator,
                RouteParserToken::Exact("dolor"),
                RouteParserToken::Separator,
                RouteParserToken::Exact("sit"),
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn capture_path() {
            let parsed = parse("/{lorem}/{ipsum}").unwrap();
            let expected = vec![
                RouteParserToken::Separator,
                RouteParserToken::Capture(RefCaptureVariant::Named("lorem")),
                RouteParserToken::Separator,
                RouteParserToken::Capture(RefCaptureVariant::Named("ipsum")),
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn query() {
            let parsed = parse("?query=this").unwrap();
            let expected = vec![
                RouteParserToken::QueryBegin,
                RouteParserToken::Query {
                    ident: "query",
                    capture_or_exact: CaptureOrExact::Exact("this"),
                },
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn query_2_part() {
            let parsed = parse("?lorem=ipsum&dolor=sit").unwrap();
            let expected = vec![
                RouteParserToken::QueryBegin,
                RouteParserToken::Query {
                    ident: "lorem",
                    capture_or_exact: CaptureOrExact::Exact("ipsum"),
                },
                RouteParserToken::QuerySeparator,
                RouteParserToken::Query {
                    ident: "dolor",
                    capture_or_exact: CaptureOrExact::Exact("sit"),
                },
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn query_3_part() {
            let parsed = parse("?lorem=ipsum&dolor=sit&amet=consectetur").unwrap();
            let expected = vec![
                RouteParserToken::QueryBegin,
                RouteParserToken::Query {
                    ident: "lorem",
                    capture_or_exact: CaptureOrExact::Exact("ipsum"),
                },
                RouteParserToken::QuerySeparator,
                RouteParserToken::Query {
                    ident: "dolor",
                    capture_or_exact: CaptureOrExact::Exact("sit"),
                },
                RouteParserToken::QuerySeparator,
                RouteParserToken::Query {
                    ident: "amet",
                    capture_or_exact: CaptureOrExact::Exact("consectetur"),
                },
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn exact_fragment() {
            let parsed = parse("#lorem").unwrap();
            let expected = vec![
                RouteParserToken::FragmentBegin,
                RouteParserToken::Exact("lorem"),
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn capture_fragment() {
            let parsed = parse("#{lorem}").unwrap();
            let expected = vec![
                RouteParserToken::FragmentBegin,
                RouteParserToken::Capture(RefCaptureVariant::Named("lorem")),
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn mixed_fragment() {
            let parsed = parse("#{lorem}ipsum{dolor}").unwrap();
            let expected = vec![
                RouteParserToken::FragmentBegin,
                RouteParserToken::Capture(RefCaptureVariant::Named("lorem")),
                RouteParserToken::Exact("ipsum"),
                RouteParserToken::Capture(RefCaptureVariant::Named("dolor")),
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn end_after_path() {
            let parsed = parse("/lorem!").unwrap();
            let expected = vec![
                RouteParserToken::Separator,
                RouteParserToken::Exact("lorem"),
                RouteParserToken::End,
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn end_after_path_separator() {
            let parsed = parse("/lorem/!").unwrap();
            let expected = vec![
                RouteParserToken::Separator,
                RouteParserToken::Exact("lorem"),
                RouteParserToken::Separator,
                RouteParserToken::End,
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn end_after_path_capture() {
            let parsed = parse("/lorem/{cap}!").unwrap();
            let expected = vec![
                RouteParserToken::Separator,
                RouteParserToken::Exact("lorem"),
                RouteParserToken::Separator,
                RouteParserToken::Capture(RefCaptureVariant::Named("cap")),
                RouteParserToken::End,
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn end_after_query_capture() {
            let parsed = parse("?lorem={cap}!").unwrap();
            let expected = vec![
                RouteParserToken::QueryBegin,
                RouteParserToken::Query {
                    ident: "lorem",
                    capture_or_exact: CaptureOrExact::Capture(RefCaptureVariant::Named("cap")),
                },
                RouteParserToken::End,
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn end_after_frag_capture() {
            let parsed = parse("#{cap}!").unwrap();
            let expected = vec![
                RouteParserToken::FragmentBegin,
                RouteParserToken::Capture(RefCaptureVariant::Named("cap")),
                RouteParserToken::End,
            ];
            assert_eq!(parsed, expected);
        }

        #[test]
        fn just_end() {
            let parsed = parse("!").unwrap();
            assert_eq!(parsed, vec![RouteParserToken::End]);
        }
    }
}
