use crate::{
    error::PrettyParseError,
    parser::{parse, CaptureOrExact, RefCaptureVariant, RouteParserToken},
};

use crate::{core::FieldType, CaptureVariant, MatcherToken};

impl<'a> From<RefCaptureVariant<'a>> for CaptureVariant {
    fn from(v: RefCaptureVariant<'a>) -> Self {
        match v {
            RefCaptureVariant::Named(s) => CaptureVariant::Named(s.to_string()),
            RefCaptureVariant::ManyNamed(s) => CaptureVariant::ManyNamed(s.to_string()),
            RefCaptureVariant::NumberedNamed { sections, name } => CaptureVariant::NumberedNamed {
                sections,
                name: name.to_string(),
            },
            RefCaptureVariant::Unnamed => CaptureVariant::Unnamed,
            RefCaptureVariant::ManyUnnamed => CaptureVariant::ManyUnnamed,
            RefCaptureVariant::NumberedUnnamed { sections } => {
                CaptureVariant::NumberedUnnamed { sections }
            }
        }
    }
}

impl<'a> From<CaptureOrExact<'a>> for MatcherToken {
    fn from(value: CaptureOrExact<'a>) -> Self {
        match value {
            CaptureOrExact::Exact(m) => MatcherToken::Exact(m.to_string()),
            CaptureOrExact::Capture(v) => MatcherToken::Capture(v.into()),
        }
    }
}

impl<'a> RouteParserToken<'a> {
    fn as_str(&self) -> &str {
        match self {
            RouteParserToken::Separator => "/",
            RouteParserToken::Exact(literal) => &literal,
            RouteParserToken::QueryBegin => "?",
            RouteParserToken::QuerySeparator => "&",
            RouteParserToken::FragmentBegin => "#",
            RouteParserToken::Capture { .. }
            | RouteParserToken::Query { .. }
            | RouteParserToken::End => unreachable!(),
        }
    }
}

/// Parse the provided "matcher string" and then optimize the tokens.
pub fn parse_str_and_optimize_tokens(
    i: &str,
    field_type: FieldType,
) -> Result<Vec<MatcherToken>, PrettyParseError> {
    let tokens = parse(i, field_type)?;
    Ok(convert_tokens(&tokens))
}

/// Converts a slice of `RouteParserToken` into a Vec of MatcherTokens.
///
/// In the process of converting the tokens, this function will condense multiple RouteParserTokens
/// that represent literals into one Exact variant if multiple reducible tokens happen to occur in a row.
pub fn convert_tokens(tokens: &[RouteParserToken]) -> Vec<MatcherToken> {
    let mut new_tokens = vec![];
    let mut run: Vec<RouteParserToken> = vec![];

    for token in tokens.iter() {
        match token {
            RouteParserToken::QueryBegin
            | RouteParserToken::FragmentBegin
            | RouteParserToken::Separator
            | RouteParserToken::QuerySeparator
            | RouteParserToken::Exact(_) => run.push(*token),
            RouteParserToken::Capture(cap) => {
                new_tokens.push(MatcherToken::Exact(
                    run.iter().map(RouteParserToken::as_str).collect(),
                ));
                run = vec![];
                new_tokens.push(MatcherToken::Capture(CaptureVariant::from(*cap)))
            }
            RouteParserToken::Query {
                ident,
                capture_or_exact,
            } => match capture_or_exact {
                CaptureOrExact::Exact(s) => {
                    run.push(RouteParserToken::Exact(ident));
                    run.push(RouteParserToken::Exact("="));
                    run.push(RouteParserToken::Exact(s));
                }
                CaptureOrExact::Capture(cap) => {
                    let sequence = run
                        .iter()
                        .map(RouteParserToken::as_str)
                        .chain(Some(*ident))
                        .chain(Some("="))
                        .collect();
                    new_tokens.push(MatcherToken::Exact(sequence));
                    run = vec![];
                    new_tokens.push(MatcherToken::Capture(CaptureVariant::from(*cap)))
                }
            },
            RouteParserToken::End => {
                let sequence = run.iter().map(RouteParserToken::as_str).collect();
                new_tokens.push(MatcherToken::Exact(sequence));
                run = vec![];
                new_tokens.push(MatcherToken::End);
            }
        }
    }

    // Empty the run at the end.
    if !run.is_empty() {
        new_tokens.push(MatcherToken::Exact(
            run.iter().map(RouteParserToken::as_str).collect(),
        ));
    }

    new_tokens
}
