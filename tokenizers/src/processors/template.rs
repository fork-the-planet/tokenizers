//! # Template Processing
//!
//! Provides a way to specify templates in order to add the special tokens to each
//! input sequence as relevant.
//!
//! ## Example
//!
//! Let's take `BERT` tokenizer as an example. It uses two special tokens, used to
//! delimitate each sequence. `[CLS]` is always used at the beginning of the first
//! sequence, and `[SEP]` is added at the end of both the first, and the pair
//! sequences. The final result looks like this:
//! - Single sequence: `[CLS] Hello there [SEP]`
//! - Pair sequences: `[CLS] My name is Anthony [SEP] What is my name? [SEP]`
//!
//! With the type ids as following:
//! ```markdown
//! [CLS]   ...   [SEP]   ...   [SEP]
//!   0      0      0      1      1
//! ```
//!
//! So, we can define a [`TemplateProcessing`] that will achieve this result:
//! ```
//! # use tokenizers::processors::template::TemplateProcessing;
//! let template = TemplateProcessing::builder()
//!     // The template when we only have a single sequence:
//!     .try_single(vec!["[CLS]", "$0", "[SEP]"]).unwrap()
//!     // Same as:
//!     .try_single("[CLS] $0 [SEP]").unwrap()
//!
//!     // The template when we have both sequences:
//!     .try_pair(vec!["[CLS]:0", "$A:0", "[SEP]:0", "$B:1", "[SEP]:1"]).unwrap()
//!     // Same as:
//!     .try_pair("[CLS]:0 $A:0 [SEP]:0 $B:1 [SEP]:1").unwrap()
//!     // Or:
//!     .try_pair("[CLS] $0 [SEP] $B:1 [SEP]:1").unwrap()
//!
//!     // The list of special tokens used by each sequences
//!     .special_tokens(vec![("[CLS]", 1), ("[SEP]", 0)])
//!     .build()
//!     .unwrap();
//! ```
//!
//! In this example, each input sequence is identified using a `$` construct. This identifier
//! lets us specify each input sequence, and the type_id to use. When nothing is specified,
//! it uses the default values. Here are the different ways to specify it:
//! - Specifying the sequence, with default `type_id == 0`: `$A` or `$B`
//! - Specifying the `type_id` with default `sequence == A`: `$0`, `$1`, `$2`, ...
//! - Specifying both: `$A:0`, `$B:1`, ...
//!
//! The same construct is used for special tokens: `<identifier>(:<type_id>)?`.
//!
//! **Warning**: You must ensure that you are giving the correct tokens/ids as these will
//! be added to the `Encoding` without any further check. If the given ids correspond to
//! something totally different in a `Tokenizer` using this `PostProcessor`, it might lead
//! to unexpected results.
//!
//! [`TemplateProcessing`]: struct.TemplateProcessing.html
//!
use crate::{Encoding, PostProcessor, Result};
use ahash::{AHashMap, AHashSet};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::result::Result as StdResult;

/// Represents any sequences received as input of the PostProcessor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
pub enum Sequence {
    /// This is the first sequence, the one that is always specified
    A,
    /// This is the pair sequence, that is optional
    B,
}

/// Represents the different kind of pieces that constitute a template.
/// It can be either the input sequence or a [`SpecialToken`]:
///
/// - The `Sequence` has an associated `type_id` which is used by default
///   for any token inside this sequence. The `Sequence` corresponds to one
///   of the input sequence given as input of the `PostProcessor`.
///
/// - The `SpecialToken` has an associated `id`. It corresponds to a [`SpecialToken`].
///
/// The easiest way to build a `Piece` is actually by converting it from a string:
/// ```
/// # use tokenizers::processors::template::Piece;
/// # use std::convert::TryFrom;
/// let sequence_with_type_id_0 = Piece::try_from("$0").unwrap();
/// let sequence_with_type_id_1 = Piece::try_from("$1").unwrap();
/// let special_token_cls = Piece::try_from("[CLS]").unwrap();
/// ```
///
/// [`SpecialToken`]: struct.SpecialToken.html
///
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
pub enum Piece {
    Sequence { id: Sequence, type_id: u32 },
    SpecialToken { id: String, type_id: u32 },
}

impl Piece {
    fn extract_id(s: &str) -> Option<Self> {
        if s.starts_with('$') {
            let rest = &s['$'.len_utf8()..];

            // If the id is just `$`, we use 0 as type_id, and Sequence A
            match rest {
                "" => Some(Self::Sequence {
                    id: Sequence::A,
                    type_id: 0,
                }),
                "A" | "a" => Some(Self::Sequence {
                    id: Sequence::A,
                    type_id: 0,
                }),
                "B" | "b" => Some(Self::Sequence {
                    id: Sequence::B,
                    type_id: 0,
                }),
                n => {
                    if let Ok(type_id) = n.parse::<u32>() {
                        Some(Self::Sequence {
                            id: Sequence::A,
                            type_id,
                        })
                    } else {
                        None
                    }
                }
            }
        } else {
            Some(Self::SpecialToken {
                id: s.to_owned(),
                type_id: 0,
            })
        }
    }

    fn with_type_id(self, type_id: u32) -> Self {
        match self {
            Self::Sequence { id, .. } => Self::Sequence { id, type_id },
            Self::SpecialToken { id, .. } => Self::SpecialToken { id, type_id },
        }
    }
}

impl TryFrom<String> for Piece {
    type Error = String;

    fn try_from(s: String) -> StdResult<Self, Self::Error> {
        let parts = s.split(':').collect::<Vec<_>>();

        let err = || format!("Cannot build Piece from string \"{s}\"");
        match parts.as_slice() {
            [id, type_id] => {
                let type_id: u32 = type_id.parse().map_err(|_| err())?;
                let piece = Self::extract_id(id).ok_or_else(err)?;
                Ok(piece.with_type_id(type_id))
            }
            [id] => Self::extract_id(id).ok_or_else(err),
            _ => Err(err()),
        }
    }
}

impl TryFrom<&str> for Piece {
    type Error = String;

    fn try_from(s: &str) -> StdResult<Self, Self::Error> {
        Piece::try_from(s.to_owned())
    }
}

/// Represents a bunch of tokens to be used in a template.
/// Usually, special tokens have only one associated id/token but in
/// some cases, it might be interesting to have multiple ids/tokens.
///
/// # Examples
/// ```
/// # use tokenizers::processors::template::SpecialToken;
/// // Simple cases, where a single id/token is necessary:
/// let cls = SpecialToken::from(("[CLS]", 1));
/// let sep = SpecialToken::from((0, "[SEP]")); // The order in the tuple is not important
///
/// // More complex case with multiple values:
/// let complex = SpecialToken::new(
///     "A complex special token:".into(),
///     vec![0, 1, 2, 3, 4],
///     vec!["A".into(), "complex".into(), "special".into(), "token".into(), ":".into()]
/// ).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
pub struct SpecialToken {
    /// A unique id used to identify this SpecialToken in the template
    id: String,
    /// The list of associated ids
    ids: Vec<u32>,
    /// The list of associated tokens
    tokens: Vec<String>,
}

impl From<(String, u32)> for SpecialToken {
    fn from(v: (String, u32)) -> Self {
        Self {
            id: v.0.clone(),
            ids: vec![v.1],
            tokens: vec![v.0],
        }
    }
}
impl From<(&str, u32)> for SpecialToken {
    fn from(v: (&str, u32)) -> Self {
        Self::from((v.0.to_owned(), v.1))
    }
}
impl From<(u32, String)> for SpecialToken {
    fn from(v: (u32, String)) -> Self {
        Self::from((v.1, v.0))
    }
}
impl From<(u32, &str)> for SpecialToken {
    fn from(v: (u32, &str)) -> Self {
        Self::from((v.1.to_owned(), v.0))
    }
}

impl SpecialToken {
    pub fn new(id: String, ids: Vec<u32>, tokens: Vec<String>) -> Result<Self> {
        if ids.len() != tokens.len() {
            Err("SpecialToken: ids and tokens must be of the same length".into())
        } else {
            Ok(Self { id, ids, tokens })
        }
    }
}

/// A Template represents a Vec<[`Piece`]>.
///
/// We can easily build one as follows
/// ```
/// # use tokenizers::processors::template::Template;
/// # use std::convert::TryFrom;
/// // By providing a `String` or `&str`, we just split on whitespaces:
/// let template = Template::try_from("[CLS] $0 [SEP]").unwrap();
///
/// // By providing pieces directly:
/// let template = Template::try_from(vec!["[CLS]", "$0", "[SEP]"]).unwrap();
/// ```
/// Both of these methods give the same result.
///
/// [`Piece`]: enum.Piece.html
///
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq)]
#[serde(transparent)]
pub struct Template(Vec<Piece>);

impl<T> TryFrom<Vec<T>> for Template
where
    T: TryInto<Piece, Error = String>,
{
    type Error = String;

    fn try_from(v: Vec<T>) -> StdResult<Self, Self::Error> {
        Ok(Self(
            v.into_iter()
                .map(|p| p.try_into())
                .collect::<StdResult<Vec<_>, Self::Error>>()?,
        ))
    }
}

impl TryFrom<String> for Template {
    type Error = String;

    fn try_from(s: String) -> StdResult<Self, Self::Error> {
        Self::try_from(s.as_ref())
    }
}

impl TryFrom<&str> for Template {
    type Error = String;

    fn try_from(s: &str) -> StdResult<Self, Self::Error> {
        Self::try_from(s.split(' ').collect::<Vec<_>>())
    }
}

/// A bunch of [`SpecialToken`] represented by their ID.
/// Internally, `Tokens` is a `HashMap<String, SpecialToken>` and can be built
/// from a HashMap or a Vec<[`SpecialToken`]>.
///
/// [`SpecialToken`]: struct.SpecialToken.html
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Eq)]
#[serde(transparent)]
pub struct Tokens(
    #[serde(serialize_with = "crate::utils::ordered_map")] pub AHashMap<String, SpecialToken>,
);

impl<T: Into<SpecialToken>> From<Vec<T>> for Tokens {
    fn from(v: Vec<T>) -> Self {
        Self(
            v.into_iter()
                .map(|t| {
                    let token: SpecialToken = t.into();
                    (token.id.clone(), token)
                })
                .collect(),
        )
    }
}

impl From<AHashMap<String, SpecialToken>> for Tokens {
    fn from(v: AHashMap<String, SpecialToken>) -> Self {
        Self(v)
    }
}

/// This PostProcessor takes care of processing each input `Encoding` by applying
/// the corresponding template, before merging them in the final Encoding.
///
/// A `Template` is actually a sequence of `Piece` that will be
/// concatenated together in the given order. Each `Piece` represents either
/// one of the input `Encoding` or a `SpecialToken`.
///
/// ## Example
/// ```
/// # use tokenizers::processors::template::TemplateProcessing;
/// let template = TemplateProcessing::builder()
///     .try_single("[CLS] $A [SEP]").unwrap()
///     .try_pair("[CLS] $A [SEP] $B:1 [SEP]:1").unwrap()
///     .special_tokens(vec![("[CLS]", 1), ("[SEP]", 0)])
///     .build()
///     .unwrap();
/// ```
///
#[derive(Debug, Clone, PartialEq, Builder, Serialize, Deserialize, Eq)]
#[serde(tag = "type", from = "TemplateProcessingDeserializer")]
#[builder(build_fn(validate = "Self::validate"))]
pub struct TemplateProcessing {
    #[builder(try_setter, default = "\"$0\".try_into().unwrap()")]
    pub single: Template,
    #[builder(try_setter, default = "\"$A:0 $B:1\".try_into().unwrap()")]
    pair: Template,
    #[builder(setter(skip), default = "self.default_added(true)")]
    #[serde(skip)]
    added_single: usize,
    #[builder(setter(skip), default = "self.default_added(false)")]
    #[serde(skip)]
    added_pair: usize,
    #[builder(setter(into), default)]
    special_tokens: Tokens,
}

impl TemplateProcessing {
    // Getter for `single`
    pub fn get_single(&self) -> String {
        format!("{:?}", self.single)
    }

    // Setter for `single`
    pub fn set_single(&mut self, single: Template) {
        self.single = single;
    }

    // Getter for `pair`
    pub fn get_pair(&self) -> &Template {
        &self.pair
    }

    // Setter for `pair`
    pub fn set_pair(&mut self, pair: Template) {
        self.pair = pair;
    }

    // Getter for `added_single`
    pub fn get_added_single(&self) -> usize {
        self.added_single
    }

    // Setter for `added_single`
    pub fn set_added_single(&mut self, added_single: usize) {
        self.added_single = added_single;
    }

    // Getter for `added_pair`
    pub fn get_added_pair(&self) -> usize {
        self.added_pair
    }

    // Setter for `added_pair`
    pub fn set_added_pair(&mut self, added_pair: usize) {
        self.added_pair = added_pair;
    }

    // Getter for `special_tokens`
    pub fn get_special_tokens(&self) -> &Tokens {
        &self.special_tokens
    }

    // Setter for `special_tokens`
    pub fn set_special_tokens(&mut self, special_tokens: Tokens) {
        self.special_tokens = special_tokens;
    }
}

impl From<&str> for TemplateProcessingBuilderError {
    fn from(e: &str) -> Self {
        e.to_string().into()
    }
}

impl PartialEq for TemplateProcessingBuilderError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

/// We use this custom deserializer to provided the values for `added_single`
/// and `added_pair` during deserialization, while not having to serialize them
#[doc(hidden)]
#[derive(Deserialize)]
#[serde(tag = "type")]
struct TemplateProcessingDeserializer {
    single: Template,
    pair: Template,
    special_tokens: Tokens,
}
impl From<TemplateProcessingDeserializer> for TemplateProcessing {
    fn from(t: TemplateProcessingDeserializer) -> Self {
        let added_single = count_added(&t.single, Some(&t.special_tokens));
        let added_pair = count_added(&t.pair, Some(&t.special_tokens));
        Self {
            single: t.single,
            pair: t.pair,
            added_single,
            added_pair,
            special_tokens: t.special_tokens,
        }
    }
}

/// Count the number of added tokens in the given template
fn count_added(container: &Template, special_tokens: Option<&Tokens>) -> usize {
    container
        .0
        .iter()
        .map(|p| match p {
            Piece::Sequence { .. } => 0,
            Piece::SpecialToken { id, .. } => {
                special_tokens.map_or(0, |spt| spt.0.get(id).map_or(0, |s| s.ids.len()))
            }
        })
        .sum()
}

impl TemplateProcessingBuilder {
    fn default_added(&self, is_single: bool) -> usize {
        let container = if is_single {
            self.single.as_ref()
        } else {
            self.pair.as_ref()
        };
        container.map_or(0, |pieces| {
            count_added(pieces, self.special_tokens.as_ref())
        })
    }

    fn validate(&self) -> std::result::Result<(), String> {
        let pair_has_both = self.pair.as_ref().is_none_or(|pair| {
            let mut has_a = false;
            let mut has_b = false;
            for piece in &pair.0 {
                if let Piece::Sequence {
                    id: Sequence::A, ..
                } = piece
                {
                    has_a = true;
                }
                if let Piece::Sequence {
                    id: Sequence::B, ..
                } = piece
                {
                    has_b = true;
                }
            }
            has_a && has_b
        });
        if !pair_has_both {
            return Err("Template for `pair` must use both sequences".into());
        }

        let check = |sp| {
            let exist = self
                .special_tokens
                .as_ref()
                .is_some_and(|map| map.0.contains_key(sp));

            match exist {
                false => Some(sp),
                true => None,
            }
        };

        let empty = [];
        let missing: AHashSet<&str> = self
            .single
            .as_ref()
            .map_or(empty.iter(), |s| s.0.iter())
            .chain(self.pair.as_ref().map_or(empty.iter(), |s| s.0.iter()))
            .filter_map(|piece| match piece {
                Piece::Sequence { .. } => None,
                Piece::SpecialToken { id, .. } => check(id.as_ref()),
            })
            .collect::<AHashSet<_>>();

        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Missing SpecialToken(s) with id(s) `{}`",
                missing.iter().join(", ")
            ))
        }
    }
}

impl Default for TemplateProcessing {
    fn default() -> Self {
        Self {
            single: "$0".try_into().unwrap(),
            pair: "$1".try_into().unwrap(),
            added_single: 0,
            added_pair: 0,
            special_tokens: Tokens::default(),
        }
    }
}

impl TemplateProcessing {
    pub fn builder() -> TemplateProcessingBuilder {
        TemplateProcessingBuilder::default()
    }

    fn apply_template(
        &self,
        template: &[Piece],
        mut encodings: Vec<Encoding>,
        add_special_tokens: bool,
    ) -> Result<Vec<Encoding>> {
        let final_encodings: Vec<Encoding> = template
            .iter()
            .flat_map(|piece| {
                match piece {
                    Piece::Sequence { id, type_id } => {
                        let i = usize::from(*id != Sequence::A);
                        let encoding = &mut encodings[i];
                        encoding.set_type_ids(vec![*type_id; encoding.len()]);
                        encoding.set_sequence_id(i);
                        Some(encoding.clone())
                    }
                    Piece::SpecialToken { id, type_id } => {
                        if add_special_tokens {
                            let tok = &self.special_tokens.0[id]; // We already checked existence above
                            let len = tok.ids.len();

                            let encoding = Encoding::new(
                                tok.ids.clone(),
                                std::iter::repeat_n(*type_id, len).collect(),
                                tok.tokens.clone(),
                                // words
                                std::iter::repeat_n(None, len).collect(),
                                // offsets
                                std::iter::repeat_n((0, 0), len).collect(),
                                // special_tokens_mask
                                std::iter::repeat_n(1, len).collect(),
                                // attention_mask
                                std::iter::repeat_n(1, len).collect(),
                                // overflowing
                                vec![],
                                // sequence_range
                                AHashMap::new(),
                            );
                            Some(encoding)
                        } else {
                            None
                        }
                    }
                }
            })
            .collect();

        //let mut pair = if encodings.len() > 1 {
        //    Some(encodings.pop().unwrap())
        //} else {
        //    None
        //};
        //let mut encoding = encodings.pop().unwrap();

        //let pair_overflowing = pair.as_mut().map_or(vec![], |e| e.take_overflowing());
        //let mut overflowing: Vec<Encoding> = encoding
        //    .take_overflowing()
        //    .iter()
        //    .map(|encoding| -> Result<Vec<Encoding>> {
        //        // 1. The pair itself
        //        let mut overflowings = self.apply_template(
        //            template,
        //            if encodings.len() > 1 {
        //                vec![encoding.clone(), encodings[1].clone()]
        //            } else {
        //                vec![encoding.clone()]
        //            },
        //            add_special_tokens,
        //        )?;

        //        // 2. Its overflowings
        //        for other_o in &pair_overflowing {
        //            overflowings.extend(self.apply_template(
        //                template,
        //                vec![encoding.clone(), other_o.clone()],
        //                add_special_tokens,
        //            )?);
        //        }

        //        Ok(overflowings)
        //    })
        //    .collect::<Result<Vec<Vec<Encoding>>>>()?
        //    .into_iter()
        //    .flatten()
        //    .collect();
        //// We also need to combine the first sequence with all other overflowings
        //overflowing.extend(
        //    pair_overflowing
        //        .into_iter()
        //        .map(|pair| {
        //            self.apply_template(template, vec![encoding.clone(), pair], add_special_tokens)
        //        })
        //        .collect::<Result<Vec<_>>>()?
        //        .into_iter()
        //        .flatten(),
        //);

        Ok(final_encodings)
    }
}

impl PostProcessor for TemplateProcessing {
    fn added_tokens(&self, is_pair: bool) -> usize {
        if is_pair {
            self.added_pair
        } else {
            self.added_single
        }
    }

    fn process_encodings(
        &self,
        encodings: Vec<Encoding>,
        add_special_tokens: bool,
    ) -> Result<Vec<Encoding>> {
        // let (encoding, pair): (Encoding, Option<Encoding>) = match encodings.len() {
        //     1 => (
        //         encodings
        //             .pop()
        //             .ok_or(ProcessorError::InvalidEncodingsVecLength)?,
        //         None,
        //     ),
        //     2 => {
        //         let pair = encodings
        //             .pop()
        //             .ok_or(ProcessorError::InvalidEncodingsVecLength)?;
        //         let encoding = encodings
        //             .pop()
        //             .ok_or(ProcessorError::InvalidEncodingsVecLength)?;
        //         (encoding, Some(pair))
        //     }
        //     _ => return Err(Box::new(ProcessorError::InvalidEncodingsVecLength)),
        // };
        let template = match encodings.len() {
            2 => &self.pair.0,
            1 => &self.single.0,
            _ => todo!(),
        };
        let encodings = self.apply_template(template, encodings, add_special_tokens)?;
        Ok(encodings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;
    use std::iter::FromIterator;

    #[test]
    fn piece_serde() {
        let seq_0 = Piece::Sequence {
            id: Sequence::A,
            type_id: 0,
        };
        let seq_0_s = r#"{"Sequence":{"id":"A","type_id":0}}"#;

        assert_eq!(serde_json::to_string(&seq_0).unwrap(), seq_0_s);
        assert_eq!(serde_json::from_str::<Piece>(seq_0_s).unwrap(), seq_0);

        let seq_1 = Piece::Sequence {
            id: Sequence::B,
            type_id: 1,
        };
        let seq_1_s = r#"{"Sequence":{"id":"B","type_id":1}}"#;
        assert_eq!(serde_json::to_string(&seq_1).unwrap(), seq_1_s);
        assert_eq!(serde_json::from_str::<Piece>(seq_1_s).unwrap(), seq_1);

        let spe = Piece::SpecialToken {
            id: "[CLS]".into(),
            type_id: 0,
        };
        let spe_s = r#"{"SpecialToken":{"id":"[CLS]","type_id":0}}"#;
        assert_eq!(serde_json::to_string(&spe).unwrap(), spe_s);
        assert_eq!(serde_json::from_str::<Piece>(spe_s).unwrap(), spe);
    }

    #[test]
    fn piece() {
        assert_eq!(
            Ok(Piece::Sequence {
                id: Sequence::A,
                type_id: 0
            }),
            "$".try_into()
        );
        assert_eq!(
            Ok(Piece::Sequence {
                id: Sequence::B,
                type_id: 0
            }),
            "$B".try_into()
        );
        assert_eq!(
            Ok(Piece::Sequence {
                id: Sequence::A,
                type_id: 1
            }),
            "$1".try_into()
        );
        assert_eq!(
            Ok(Piece::Sequence {
                id: Sequence::B,
                type_id: 2
            }),
            "$B:2".try_into()
        );
        assert_eq!(
            Ok(Piece::Sequence {
                id: Sequence::A,
                type_id: 1
            }),
            "$:1".try_into()
        );
        assert!(Piece::try_from("$C:1").is_err());
        assert!(Piece::try_from("$A:").is_err());
    }

    #[test]
    fn special_token_serde() {
        let simple = SpecialToken::from(("[CLS]", 0));
        let simple_s = r#"{"id":"[CLS]","ids":[0],"tokens":["[CLS]"]}"#;
        assert_eq!(serde_json::to_string(&simple).unwrap(), simple_s);
        assert_eq!(
            serde_json::from_str::<SpecialToken>(simple_s).unwrap(),
            simple
        );

        let complete = SpecialToken::new(
            "[2FR]".into(),
            vec![1, 2, 3],
            vec!["convert".into(), "to".into(), "FR".into()],
        )
        .unwrap();
        let complete_s = r#"{"id":"[2FR]","ids":[1,2,3],"tokens":["convert","to","FR"]}"#;
        assert_eq!(serde_json::to_string(&complete).unwrap(), complete_s);
        assert_eq!(
            serde_json::from_str::<SpecialToken>(complete_s).unwrap(),
            complete
        );

        let malformed = SpecialToken::new(
            "[2FR]".into(),
            vec![1, 2],
            vec!["convert".into(), "to".into(), "FR".into()],
        );
        assert!(malformed.is_err());
        let malformed = SpecialToken::new(
            "[2FR]".into(),
            vec![1, 2, 3],
            vec!["convert".into(), "FR".into()],
        );
        assert!(malformed.is_err());
    }

    #[test]
    fn template_serde() {
        let template = Template(vec![
            Piece::Sequence {
                id: Sequence::A,
                type_id: 0,
            },
            Piece::SpecialToken {
                id: "[CLS]".into(),
                type_id: 0,
            },
        ]);
        let template_s =
            r#"[{"Sequence":{"id":"A","type_id":0}},{"SpecialToken":{"id":"[CLS]","type_id":0}}]"#;
        assert_eq!(serde_json::to_string(&template).unwrap(), template_s);
        assert_eq!(
            serde_json::from_str::<Template>(template_s).unwrap(),
            template
        );
    }

    #[test]
    fn tokens_serde() {
        let tokens = Tokens::from(vec![("[CLS]", 1), ("[SEP]", 0)]);
        let tokens_s = r#"{"[CLS]":{"id":"[CLS]","ids":[1],"tokens":["[CLS]"]},"[SEP]":{"id":"[SEP]","ids":[0],"tokens":["[SEP]"]}}"#;
        let tokens_ser = serde_json::to_string(&tokens).unwrap();
        assert_eq!(tokens_ser, tokens_s);
        assert_eq!(serde_json::from_str::<Tokens>(tokens_s).unwrap(), tokens);
    }

    fn get_bert_template() -> TemplateProcessing {
        TemplateProcessing::builder()
            .try_single(vec!["[CLS]", "$0", "[SEP]"])
            .unwrap()
            .try_pair("[CLS]:0 $A:0 [SEP]:0 $B:1 [SEP]:1")
            .unwrap()
            .special_tokens(vec![("[CLS]", 1), ("[SEP]", 0)])
            .build()
            .unwrap()
    }

    #[test]
    fn template_processing_serde() {
        let template = tests::get_bert_template();
        let template_s = "{\
            \"type\":\"TemplateProcessing\",\
            \"single\":[\
                {\"SpecialToken\":{\"id\":\"[CLS]\",\"type_id\":0}},\
                {\"Sequence\":{\"id\":\"A\",\"type_id\":0}},\
                {\"SpecialToken\":{\"id\":\"[SEP]\",\"type_id\":0}}\
            ],\
            \"pair\":[\
                {\"SpecialToken\":{\"id\":\"[CLS]\",\"type_id\":0}},\
                {\"Sequence\":{\"id\":\"A\",\"type_id\":0}},\
                {\"SpecialToken\":{\"id\":\"[SEP]\",\"type_id\":0}},\
                {\"Sequence\":{\"id\":\"B\",\"type_id\":1}},\
                {\"SpecialToken\":{\"id\":\"[SEP]\",\"type_id\":1}}\
            ],\
            \"special_tokens\":{\
                \"[CLS]\":{\
                    \"id\":\"[CLS]\",\"ids\":[1],\"tokens\":[\"[CLS]\"]\
                },\
                \"[SEP]\":{\
                    \"id\":\"[SEP]\",\"ids\":[0],\"tokens\":[\"[SEP]\"]\
                }\
            }}";
        let template_ser = serde_json::to_string(&template).unwrap();
        assert_eq!(template_ser, template_s);
        assert_eq!(
            serde_json::from_str::<TemplateProcessing>(template_s).unwrap(),
            template
        );
    }

    #[test]
    fn missing_special_tokens() {
        let processor = TemplateProcessing::builder()
            .try_single("[CLS] $0 [SEP]")
            .unwrap()
            .try_pair("[CLS] $A:0 [SEP] $B:1 [SEP]")
            .unwrap()
            .build();

        let err_a = Err("Missing SpecialToken(s) with id(s) `[SEP], [CLS]`".into());
        let err_b = Err("Missing SpecialToken(s) with id(s) `[CLS], [SEP]`".into());
        assert!(processor == err_a || processor == err_b);
    }

    #[test]
    fn template_processing() {
        let processor = tests::get_bert_template();
        assert_eq!(processor.added_tokens(false), 2);
        assert_eq!(processor.added_tokens(true), 3);

        use crate::Token;
        let encoding = Encoding::from_tokens(
            vec![
                Token::new(12, "Hello".into(), (0, 5)),
                Token::new(14, "there".into(), (6, 11)),
            ],
            0,
        );
        let pair = Encoding::from_tokens(vec![Token::new(15, "pair".into(), (0, 4))], 0);
        let single_encoding = processor.process(encoding.clone(), None, true).unwrap();
        assert_eq!(
            single_encoding,
            Encoding::new(
                vec![1, 12, 14, 0],
                vec![0, 0, 0, 0],
                vec![
                    "[CLS]".into(),
                    "Hello".into(),
                    "there".into(),
                    "[SEP]".into()
                ],
                vec![None, None, None, None],
                vec![(0, 0), (0, 5), (6, 11), (0, 0)],
                vec![1, 0, 0, 1],
                vec![1, 1, 1, 1],
                vec![],
                AHashMap::from_iter(vec![(0, 1..3)]),
            )
        );
        assert_eq!(single_encoding.token_to_sequence(2), Some(0));
        assert_eq!(single_encoding.token_to_sequence(3), None);
        let pair_encoding = processor.process(encoding, Some(pair), true).unwrap();
        assert_eq!(
            pair_encoding,
            Encoding::new(
                vec![1, 12, 14, 0, 15, 0],
                vec![0, 0, 0, 0, 1, 1],
                vec![
                    "[CLS]".into(),
                    "Hello".into(),
                    "there".into(),
                    "[SEP]".into(),
                    "pair".into(),
                    "[SEP]".into()
                ],
                vec![None, None, None, None, None, None],
                vec![(0, 0), (0, 5), (6, 11), (0, 0), (0, 4), (0, 0)],
                vec![1, 0, 0, 1, 0, 1],
                vec![1, 1, 1, 1, 1, 1],
                vec![],
                AHashMap::from_iter(vec![(0, 1..3), (1, 4..5)]),
            )
        );
        assert_eq!(pair_encoding.token_to_sequence(2), Some(0));
        assert_eq!(pair_encoding.token_to_sequence(3), None);
        assert_eq!(pair_encoding.token_to_sequence(4), Some(1));
        assert_eq!(pair_encoding.token_to_sequence(5), None);
    }

    #[test]
    fn template_processing_overflowing() {
        let processor = tests::get_bert_template();
        assert_eq!(processor.added_tokens(false), 2);
        assert_eq!(processor.added_tokens(true), 3);

        use crate::Token;
        let mut encoding = Encoding::from_tokens(
            vec![
                Token::new(12, "Hello".into(), (0, 5)),
                Token::new(14, "there".into(), (6, 11)),
            ],
            0,
        );
        let overflowing = Encoding::from_tokens(vec![Token::new(13, "you".into(), (12, 15))], 0);
        encoding.set_overflowing(vec![overflowing]);

        let mut pair = Encoding::from_tokens(
            vec![
                Token::new(15, "pair".into(), (0, 4)),
                Token::new(16, "with".into(), (5, 9)),
            ],
            0,
        );
        let pair_overflowing =
            Encoding::from_tokens(vec![Token::new(17, "info".into(), (10, 14))], 0);
        pair.set_overflowing(vec![pair_overflowing]);

        let single_encoding = processor.process(encoding.clone(), None, true).unwrap();
        assert_eq!(
            single_encoding,
            Encoding::new(
                vec![1, 12, 14, 0],
                vec![0, 0, 0, 0],
                vec![
                    "[CLS]".into(),
                    "Hello".into(),
                    "there".into(),
                    "[SEP]".into()
                ],
                vec![None, None, None, None],
                vec![(0, 0), (0, 5), (6, 11), (0, 0)],
                vec![1, 0, 0, 1],
                vec![1, 1, 1, 1],
                vec![Encoding::new(
                    vec![1, 13, 0],
                    vec![0, 0, 0],
                    vec!["[CLS]".into(), "you".into(), "[SEP]".into()],
                    vec![None, None, None],
                    vec![(0, 0), (12, 15), (0, 0)],
                    vec![1, 0, 1],
                    vec![1, 1, 1],
                    vec![],
                    AHashMap::from_iter(vec![(0, 1..2)]),
                )],
                AHashMap::from_iter(vec![(0, 1..3)]),
            )
        );
        assert_eq!(single_encoding.token_to_sequence(2), Some(0));
        assert_eq!(single_encoding.token_to_sequence(3), None);
        let pair_encoding = processor.process(encoding, Some(pair), true).unwrap();
        println!("{pair_encoding:#?}");
        assert_eq!(
            pair_encoding,
            Encoding::new(
                vec![1, 12, 14, 0, 15, 16, 0],
                vec![0, 0, 0, 0, 1, 1, 1],
                vec![
                    "[CLS]".into(),
                    "Hello".into(),
                    "there".into(),
                    "[SEP]".into(),
                    "pair".into(),
                    "with".into(),
                    "[SEP]".into()
                ],
                vec![None, None, None, None, None, None, None],
                vec![(0, 0), (0, 5), (6, 11), (0, 0), (0, 4), (5, 9), (0, 0)],
                vec![1, 0, 0, 1, 0, 0, 1],
                vec![1, 1, 1, 1, 1, 1, 1],
                vec![
                    Encoding::new(
                        vec![1, 13, 0, 15, 16, 0],
                        vec![0, 0, 0, 1, 1, 1],
                        vec![
                            "[CLS]".into(),
                            "you".into(),
                            "[SEP]".into(),
                            "pair".into(),
                            "with".into(),
                            "[SEP]".into()
                        ],
                        vec![None, None, None, None, None, None],
                        vec![(0, 0), (12, 15), (0, 0), (0, 4), (5, 9), (0, 0)],
                        vec![1, 0, 1, 0, 0, 1],
                        vec![1, 1, 1, 1, 1, 1],
                        vec![Encoding::new(
                            vec![1, 13, 0, 17, 0],
                            vec![0, 0, 0, 0, 1],
                            vec![
                                "[CLS]".into(),
                                "you".into(),
                                "[SEP]".into(),
                                "info".into(),
                                "[SEP]".into()
                            ],
                            vec![None, None, None, None, None,],
                            vec![(0, 0), (12, 15), (0, 0), (10, 14), (0, 0)],
                            vec![1, 0, 1, 0, 1],
                            vec![1, 1, 1, 1, 1],
                            vec![],
                            AHashMap::from_iter(vec![(0, 1..2), (1, 3..4)]),
                        ),],
                        AHashMap::from_iter(vec![(1, 3..5), (0, 1..2)]),
                    ),
                    Encoding::new(
                        vec![1, 13, 0, 17, 0],
                        vec![0, 0, 0, 0, 1],
                        vec![
                            "[CLS]".into(),
                            "you".into(),
                            "[SEP]".into(),
                            "info".into(),
                            "[SEP]".into()
                        ],
                        vec![None, None, None, None, None,],
                        vec![(0, 0), (12, 15), (0, 0), (10, 14), (0, 0)],
                        vec![1, 0, 1, 0, 1],
                        vec![1, 1, 1, 1, 1],
                        vec![],
                        AHashMap::from_iter(vec![(0, 1..2), (1, 3..4)]),
                    ),
                    Encoding::new(
                        vec![1, 12, 14, 0, 17, 0],
                        vec![0, 0, 0, 0, 0, 1],
                        vec![
                            "[CLS]".into(),
                            "Hello".into(),
                            "there".into(),
                            "[SEP]".into(),
                            "info".into(),
                            "[SEP]".into()
                        ],
                        vec![None, None, None, None, None, None],
                        vec![(0, 0), (0, 5), (6, 11), (0, 0), (10, 14), (0, 0)],
                        vec![1, 0, 0, 1, 0, 1],
                        vec![1, 1, 1, 1, 1, 1],
                        vec![Encoding::new(
                            vec![1, 13, 0, 17, 0],
                            vec![0, 0, 0, 0, 1],
                            vec![
                                "[CLS]".into(),
                                "you".into(),
                                "[SEP]".into(),
                                "info".into(),
                                "[SEP]".into()
                            ],
                            vec![None, None, None, None, None,],
                            vec![(0, 0), (12, 15), (0, 0), (10, 14), (0, 0)],
                            vec![1, 0, 1, 0, 1],
                            vec![1, 1, 1, 1, 1],
                            vec![],
                            AHashMap::from_iter(vec![(0, 1..2), (1, 3..4)]),
                        ),],
                        AHashMap::from_iter(vec![(0, 1..3), (1, 4..5)]),
                    )
                ],
                AHashMap::from_iter(vec![(0, 1..3), (1, 4..6)]),
            )
        );
        assert_eq!(pair_encoding.token_to_sequence(2), Some(0));
        assert_eq!(pair_encoding.token_to_sequence(3), None);
        assert_eq!(pair_encoding.token_to_sequence(4), Some(1));
        assert_eq!(pair_encoding.token_to_sequence(5), Some(1));
        assert_eq!(pair_encoding.token_to_sequence(6), None);
    }
    #[test]
    fn pair_must_use_both_sequences() {
        let processor = TemplateProcessing::builder()
            .try_single("$0")
            .unwrap()
            .try_pair("$0 $1")
            .unwrap()
            .build();
        assert_eq!(
            processor,
            Err("Template for `pair` must use both sequences".into())
        );
    }

    #[test]
    fn expect_wrong_error_message() {
        let processor = TemplateProcessing::builder()
            .try_single("$0")
            .unwrap()
            .try_pair("$0 $1")
            .unwrap()
            .build();
        assert_ne!(
            processor,
            Err("Expect the left side error message to be different from the right side!".into())
        );
    }
}
