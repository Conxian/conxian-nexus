#!/usr/bin/env ruby
# frozen_string_literal: true

require "yaml"

workflow_paths = Dir[File.join(".github", "workflows", "*.{yml,yaml}")].sort
abort("No workflow YAML files found") if workflow_paths.empty?

errors = []
workflow_paths.each do |path|
  text = File.read(path, encoding: "UTF-8")
  if text.lines.any? { |line| line.match?(/^(<<<<<<<|=======|>>>>>>>)(?:\s|$)/) }
    errors << "#{path}: unresolved conflict marker"
    next
  end

  begin
    document = YAML.safe_load(text, permitted_classes: [], permitted_symbols: [], aliases: false)
    errors << "#{path}: workflow must parse to a mapping" unless document.is_a?(Hash)
  rescue Psych::SyntaxError => e
    errors << "#{path}: YAML parse error at line #{e.line}: #{e.problem}"
  end
end

unless errors.empty?
  warn "Workflow YAML validation failed:"
  errors.each { |error| warn "- #{error}" }
  exit 1
end

puts "Parsed #{workflow_paths.length} workflow YAML files with no conflict markers."
