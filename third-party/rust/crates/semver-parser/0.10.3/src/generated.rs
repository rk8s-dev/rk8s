//! This is @generated code, do not edit by hand.
//! See `semver.pest` and `genpest.rs`.
#![allow(unused_attributes)]
use super::SemverParser;

#[allow(non_upper_case_globals)]
const _PEST_GRAMMAR_SemverParser: [&'static str; 0usize] = [];
#[allow(dead_code, non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Rule {
  #[doc = "End-of-input"]
  EOI,
  r#range_set,
  r#logical_or,
  r#range,
  r#empty,
  r#hyphen,
  r#simple,
  r#primitive,
  r#primitive_op,
  r#partial,
  r#xr,
  r#xr_op,
  r#nr,
  r#tilde,
  r#caret,
  r#qualifier,
  r#parts,
  r#part,
  r#space,
}
impl Rule {
  pub fn all_rules() -> &'static [Rule] {
    &[
      Rule::r#range_set,
      Rule::r#logical_or,
      Rule::r#range,
      Rule::r#empty,
      Rule::r#hyphen,
      Rule::r#simple,
      Rule::r#primitive,
      Rule::r#primitive_op,
      Rule::r#partial,
      Rule::r#xr,
      Rule::r#xr_op,
      Rule::r#nr,
      Rule::r#tilde,
      Rule::r#caret,
      Rule::r#qualifier,
      Rule::r#parts,
      Rule::r#part,
      Rule::r#space,
    ]
  }
}
#[allow(clippy::all)]
impl ::pest::Parser<Rule> for SemverParser {
  fn parse<'i>(
    rule: Rule,
    input: &'i str,
  ) -> ::std::result::Result<::pest::iterators::Pairs<'i, Rule>, ::pest::error::Error<Rule>> {
    mod rules {
      #![allow(clippy::upper_case_acronyms)]
      pub mod hidden {
        use super::super::Rule;
        #[inline]
        #[allow(dead_code, non_snake_case, unused_variables)]
        pub fn skip(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          Ok(state)
        }
      }
      pub mod visible {
        use super::super::Rule;
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#range_set(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#range_set, |state| {
            state.sequence(|state| {
              self::r#SOI(state)
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    state.optional(|state| {
                      self::r#space(state).and_then(|state| {
                        state.repeat(|state| {
                          state.sequence(|state| {
                            super::hidden::skip(state).and_then(|state| self::r#space(state))
                          })
                        })
                      })
                    })
                  })
                })
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| self::r#range(state))
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    state.optional(|state| {
                      state
                        .sequence(|state| {
                          self::r#logical_or(state)
                            .and_then(|state| super::hidden::skip(state))
                            .and_then(|state| self::r#range(state))
                        })
                        .and_then(|state| {
                          state.repeat(|state| {
                            state.sequence(|state| {
                              super::hidden::skip(state).and_then(|state| {
                                state.sequence(|state| {
                                  self::r#logical_or(state)
                                    .and_then(|state| super::hidden::skip(state))
                                    .and_then(|state| self::r#range(state))
                                })
                              })
                            })
                          })
                        })
                    })
                  })
                })
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    state.optional(|state| {
                      self::r#space(state).and_then(|state| {
                        state.repeat(|state| {
                          state.sequence(|state| {
                            super::hidden::skip(state).and_then(|state| self::r#space(state))
                          })
                        })
                      })
                    })
                  })
                })
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| self::r#EOI(state))
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#logical_or(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#logical_or, |state| {
            state.sequence(|state| {
              state
                .sequence(|state| {
                  state.optional(|state| {
                    self::r#space(state).and_then(|state| {
                      state.repeat(|state| {
                        state.sequence(|state| {
                          super::hidden::skip(state).and_then(|state| self::r#space(state))
                        })
                      })
                    })
                  })
                })
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| state.match_string("||"))
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    state.optional(|state| {
                      self::r#space(state).and_then(|state| {
                        state.repeat(|state| {
                          state.sequence(|state| {
                            super::hidden::skip(state).and_then(|state| self::r#space(state))
                          })
                        })
                      })
                    })
                  })
                })
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#range(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#range, |state| {
            self::r#hyphen(state)
              .or_else(|state| {
                state.sequence(|state| {
                  self::r#simple(state)
                    .and_then(|state| super::hidden::skip(state))
                    .and_then(|state| {
                      state.sequence(|state| {
                        state.optional(|state| {
                          state
                            .sequence(|state| {
                              state
                                .optional(|state| state.match_string(","))
                                .and_then(|state| super::hidden::skip(state))
                                .and_then(|state| {
                                  state.sequence(|state| {
                                    self::r#space(state)
                                      .and_then(|state| super::hidden::skip(state))
                                      .and_then(|state| {
                                        state.sequence(|state| {
                                          state.optional(|state| {
                                            self::r#space(state).and_then(|state| {
                                              state.repeat(|state| {
                                                state.sequence(|state| {
                                                  super::hidden::skip(state)
                                                    .and_then(|state| self::r#space(state))
                                                })
                                              })
                                            })
                                          })
                                        })
                                      })
                                  })
                                })
                                .and_then(|state| super::hidden::skip(state))
                                .and_then(|state| self::r#simple(state))
                            })
                            .and_then(|state| {
                              state.repeat(|state| {
                                state.sequence(|state| {
                                  super::hidden::skip(state).and_then(|state| {
                                    state.sequence(|state| {
                                      state
                                        .optional(|state| state.match_string(","))
                                        .and_then(|state| super::hidden::skip(state))
                                        .and_then(|state| {
                                          state.sequence(|state| {
                                            self::r#space(state)
                                              .and_then(|state| super::hidden::skip(state))
                                              .and_then(|state| {
                                                state.sequence(|state| {
                                                  state.optional(|state| {
                                                    self::r#space(state).and_then(|state| {
                                                      state.repeat(|state| {
                                                        state.sequence(|state| {
                                                          super::hidden::skip(state)
                                                            .and_then(|state| self::r#space(state))
                                                        })
                                                      })
                                                    })
                                                  })
                                                })
                                              })
                                          })
                                        })
                                        .and_then(|state| super::hidden::skip(state))
                                        .and_then(|state| self::r#simple(state))
                                    })
                                  })
                                })
                              })
                            })
                        })
                      })
                    })
                })
              })
              .or_else(|state| self::r#empty(state))
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#empty(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#empty, |state| state.match_string(""))
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#hyphen(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#hyphen, |state| {
            state.sequence(|state| {
              self::r#partial(state)
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    self::r#space(state)
                      .and_then(|state| super::hidden::skip(state))
                      .and_then(|state| {
                        state.sequence(|state| {
                          state.optional(|state| {
                            self::r#space(state).and_then(|state| {
                              state.repeat(|state| {
                                state.sequence(|state| {
                                  super::hidden::skip(state).and_then(|state| self::r#space(state))
                                })
                              })
                            })
                          })
                        })
                      })
                  })
                })
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| state.match_string("-"))
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    self::r#space(state)
                      .and_then(|state| super::hidden::skip(state))
                      .and_then(|state| {
                        state.sequence(|state| {
                          state.optional(|state| {
                            self::r#space(state).and_then(|state| {
                              state.repeat(|state| {
                                state.sequence(|state| {
                                  super::hidden::skip(state).and_then(|state| self::r#space(state))
                                })
                              })
                            })
                          })
                        })
                      })
                  })
                })
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| self::r#partial(state))
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#simple(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#simple, |state| {
            self::r#primitive(state)
              .or_else(|state| self::r#partial(state))
              .or_else(|state| self::r#tilde(state))
              .or_else(|state| self::r#caret(state))
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#primitive(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#primitive, |state| {
            state.sequence(|state| {
              self::r#primitive_op(state)
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    state.optional(|state| {
                      self::r#space(state).and_then(|state| {
                        state.repeat(|state| {
                          state.sequence(|state| {
                            super::hidden::skip(state).and_then(|state| self::r#space(state))
                          })
                        })
                      })
                    })
                  })
                })
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| self::r#partial(state))
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#primitive_op(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#primitive_op, |state| {
            state
              .match_string("<=")
              .or_else(|state| state.match_string(">="))
              .or_else(|state| state.match_string(">"))
              .or_else(|state| state.match_string("<"))
              .or_else(|state| state.match_string("="))
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#partial(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#partial, |state| {
            state.sequence(|state| {
              self::r#xr(state)
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.optional(|state| {
                    state.sequence(|state| {
                      state
                        .match_string(".")
                        .and_then(|state| super::hidden::skip(state))
                        .and_then(|state| self::r#xr(state))
                        .and_then(|state| super::hidden::skip(state))
                        .and_then(|state| {
                          state.optional(|state| {
                            state.sequence(|state| {
                              state
                                .match_string(".")
                                .and_then(|state| super::hidden::skip(state))
                                .and_then(|state| self::r#xr(state))
                                .and_then(|state| super::hidden::skip(state))
                                .and_then(|state| state.optional(|state| self::r#qualifier(state)))
                            })
                          })
                        })
                    })
                  })
                })
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#xr(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#xr, |state| {
            self::r#xr_op(state).or_else(|state| self::r#nr(state))
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#xr_op(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#xr_op, |state| {
            state
              .match_string("x")
              .or_else(|state| state.match_string("X"))
              .or_else(|state| state.match_string("*"))
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#nr(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#nr, |state| {
            state.match_string("0").or_else(|state| {
              state.sequence(|state| {
                state
                  .match_range('1'..'9')
                  .and_then(|state| super::hidden::skip(state))
                  .and_then(|state| {
                    state.sequence(|state| {
                      state.optional(|state| {
                        state.match_range('0'..'9').and_then(|state| {
                          state.repeat(|state| {
                            state.sequence(|state| {
                              super::hidden::skip(state)
                                .and_then(|state| state.match_range('0'..'9'))
                            })
                          })
                        })
                      })
                    })
                  })
              })
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#tilde(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#tilde, |state| {
            state.sequence(|state| {
              state
                .match_string("~>")
                .or_else(|state| state.match_string("~"))
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    state.optional(|state| {
                      self::r#space(state).and_then(|state| {
                        state.repeat(|state| {
                          state.sequence(|state| {
                            super::hidden::skip(state).and_then(|state| self::r#space(state))
                          })
                        })
                      })
                    })
                  })
                })
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| self::r#partial(state))
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#caret(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#caret, |state| {
            state.sequence(|state| {
              state
                .match_string("^")
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    state.optional(|state| {
                      self::r#space(state).and_then(|state| {
                        state.repeat(|state| {
                          state.sequence(|state| {
                            super::hidden::skip(state).and_then(|state| self::r#space(state))
                          })
                        })
                      })
                    })
                  })
                })
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| self::r#partial(state))
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#qualifier(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#qualifier, |state| {
            state.sequence(|state| {
              state
                .match_string("-")
                .or_else(|state| state.match_string("+"))
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| self::r#parts(state))
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#parts(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#parts, |state| {
            state.sequence(|state| {
              self::r#part(state)
                .and_then(|state| super::hidden::skip(state))
                .and_then(|state| {
                  state.sequence(|state| {
                    state.optional(|state| {
                      state
                        .sequence(|state| {
                          state
                            .match_string(".")
                            .and_then(|state| super::hidden::skip(state))
                            .and_then(|state| self::r#part(state))
                        })
                        .and_then(|state| {
                          state.repeat(|state| {
                            state.sequence(|state| {
                              super::hidden::skip(state).and_then(|state| {
                                state.sequence(|state| {
                                  state
                                    .match_string(".")
                                    .and_then(|state| super::hidden::skip(state))
                                    .and_then(|state| self::r#part(state))
                                })
                              })
                            })
                          })
                        })
                    })
                  })
                })
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#part(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::r#part, |state| {
            self::r#nr(state).or_else(|state| {
              state.sequence(|state| {
                state
                  .match_string("-")
                  .or_else(|state| state.match_range('0'..'9'))
                  .or_else(|state| state.match_range('A'..'Z'))
                  .or_else(|state| state.match_range('a'..'z'))
                  .and_then(|state| super::hidden::skip(state))
                  .and_then(|state| {
                    state.sequence(|state| {
                      state.optional(|state| {
                        state
                          .match_string("-")
                          .or_else(|state| state.match_range('0'..'9'))
                          .or_else(|state| state.match_range('A'..'Z'))
                          .or_else(|state| state.match_range('a'..'z'))
                          .and_then(|state| {
                            state.repeat(|state| {
                              state.sequence(|state| {
                                super::hidden::skip(state).and_then(|state| {
                                  state
                                    .match_string("-")
                                    .or_else(|state| state.match_range('0'..'9'))
                                    .or_else(|state| state.match_range('A'..'Z'))
                                    .or_else(|state| state.match_range('a'..'z'))
                                })
                              })
                            })
                          })
                      })
                    })
                  })
              })
            })
          })
        }
        #[inline]
        #[allow(non_snake_case, unused_variables)]
        pub fn r#space(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state
            .match_string(" ")
            .or_else(|state| state.match_string("\t"))
        }
        #[inline]
        #[allow(dead_code, non_snake_case, unused_variables)]
        pub fn EOI(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.rule(Rule::EOI, |state| state.end_of_input())
        }
        #[inline]
        #[allow(dead_code, non_snake_case, unused_variables)]
        pub fn SOI(
          state: ::std::boxed::Box<::pest::ParserState<'_, Rule>>,
        ) -> ::pest::ParseResult<::std::boxed::Box<::pest::ParserState<'_, Rule>>> {
          state.start_of_input()
        }
      }
      pub use self::visible::*;
    }
    ::pest::state(input, |state| match rule {
      Rule::r#range_set => rules::r#range_set(state),
      Rule::r#logical_or => rules::r#logical_or(state),
      Rule::r#range => rules::r#range(state),
      Rule::r#empty => rules::r#empty(state),
      Rule::r#hyphen => rules::r#hyphen(state),
      Rule::r#simple => rules::r#simple(state),
      Rule::r#primitive => rules::r#primitive(state),
      Rule::r#primitive_op => rules::r#primitive_op(state),
      Rule::r#partial => rules::r#partial(state),
      Rule::r#xr => rules::r#xr(state),
      Rule::r#xr_op => rules::r#xr_op(state),
      Rule::r#nr => rules::r#nr(state),
      Rule::r#tilde => rules::r#tilde(state),
      Rule::r#caret => rules::r#caret(state),
      Rule::r#qualifier => rules::r#qualifier(state),
      Rule::r#parts => rules::r#parts(state),
      Rule::r#part => rules::r#part(state),
      Rule::r#space => rules::r#space(state),
      Rule::EOI => rules::EOI(state),
    })
  }
}
