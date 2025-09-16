// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

/// The internal Pest parser.
#[derive(Parser)]
#[grammar = "dockerfile_parser.pest"]
pub(crate) struct DockerfileParser;

/// A Pest Pair for Dockerfile rules.
pub(crate) type Pair<'a> = pest::iterators::Pair<'a, Rule>;
