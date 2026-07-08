#!/usr/bin/env ruby
# frozen_string_literal: true

require "csv"
require "json"
require "optparse"
require "rexml/document"
require "shellwords"
require "time"

ROOT = File.expand_path("..", __dir__)

options = {
  instance_url: ENV.fetch("DAIS_OWNER_INSTANCE_URL", "https://social.dais.social"),
  apply: false,
  output: nil,
  cli: ENV["DAIS_CLI"]
}

parser = OptionParser.new do |opts|
  opts.banner = "Usage: scripts/import-preview.rb --format FORMAT --file FILE [--apply]"
  opts.on("--format FORMAT", "opml, rss-list, mastodon-following-csv, bluesky-follows, bluesky-starter-pack, local-posts-json, mastodon-outbox-json") { |v| options[:format] = v }
  opts.on("--file FILE", "Input export/archive file") { |v| options[:file] = v }
  opts.on("--instance-url URL", "Owner API instance URL") { |v| options[:instance_url] = v }
  opts.on("--owner-token TOKEN", "Owner API bearer token for --apply") { |v| options[:owner_token] = v }
  opts.on("--owner-token-file FILE", "Owner API bearer token file for --apply") { |v| options[:owner_token_file] = v }
  opts.on("--output FILE", "Write JSON plan to FILE") { |v| options[:output] = v }
  opts.on("--cli COMMAND", "Dais CLI command prefix; default uses cargo run client") { |v| options[:cli] = v }
  opts.on("--apply", "Apply supported actions through the owner API") { options[:apply] = true }
  opts.on("-h", "--help", "Show help") do
    puts opts
    exit
  end
end
parser.parse!

unless options[:format] && options[:file]
  warn parser
  exit 2
end
unless File.file?(options[:file])
  warn "Input file not found: #{options[:file]}"
  exit 2
end

def strip_html(value)
  value.to_s.gsub(/<[^>]*>/, " ").gsub(/\s+/, " ").strip
end

def command_for(action)
  case action.fetch(:action)
  when "source_add"
    [
      "owner", "source-add", action.fetch(:source_type), action.fetch(:url),
      "--title", action.fetch(:title, action.fetch(:url)),
      "--private-reader-only", "--excerpt-only", "--link-required", "--attribution-required"
    ]
  when "watch_add"
    [
      "owner", "watch-add", action.fetch(:watch_type), action.fetch(:target),
      "--title", action.fetch(:title, action.fetch(:target)),
      "--private-reader-only", "--excerpt-only", "--link-required", "--attribution-required"
    ]
  when "follow"
    ["owner", "follow", action.fetch(:target)]
  when "post_create"
    ["owner", "post-create", action.fetch(:text), "--visibility", action.fetch(:visibility, "followers")]
  else
    raise "Unsupported action: #{action.fetch(:action)}"
  end
end

def normalize_ap_handle(value)
  target = value.to_s.strip
  return target if target.empty? || target.start_with?("http://", "https://", "@")

  "@#{target}"
end

def opml_actions(path)
  document = REXML::Document.new(File.read(path))
  actions = []
  REXML::XPath.each(document, "//outline") do |outline|
    url = outline.attributes["xmlUrl"].to_s.strip
    next if url.empty?

    title = outline.attributes["title"].to_s.strip
    title = outline.attributes["text"].to_s.strip if title.empty?
    source_type = outline.attributes["type"].to_s.downcase == "atom" ? "atom" : "rss"
    actions << {
      action: "source_add",
      source_type: source_type,
      url: url,
      title: title.empty? ? url : title
    }
  end
  actions
end

def rss_list_actions(path)
  File.readlines(path, chomp: true).map do |line|
    row = line.strip
    next if row.empty? || row.start_with?("#")

    url, title = row.split(/\t+/, 2)
    {
      action: "source_add",
      source_type: "rss",
      url: url.strip,
      title: title.to_s.strip.empty? ? url.strip : title.strip
    }
  end.compact
end

def mastodon_following_actions(path)
  rows = CSV.read(path, headers: true)
  rows.map do |row|
    target = row["Account address"] || row["account"] || row["acct"] || row[0]
    target = normalize_ap_handle(target)
    next if target.empty?

    {
      action: "follow",
      target: target,
      title: target
    }
  end.compact
end

def bluesky_list_actions(path)
  text = File.read(path)
  handles = if text.lstrip.start_with?("{", "[")
    json = JSON.parse(text)
    case json
    when Array then json
    when Hash
      json["handles"] || json["actors"] || json["members"] || []
    else []
    end
  else
    CSV.parse(text).map { |row| row[0] }
  end
  handles.map do |value|
    target = value.is_a?(Hash) ? (value["handle"] || value["did"] || value["actor"] || value["target"]) : value
    target = target.to_s.strip
    next if target.empty? || target.start_with?("#")

    {
      action: "watch_add",
      watch_type: "bluesky_actor",
      target: target,
      title: target
    }
  end.compact
end

def local_posts_actions(path)
  json = JSON.parse(File.read(path))
  items = json.is_a?(Array) ? json : (json["items"] || json["posts"] || json["orderedItems"] || [])
  items.map do |item|
    object = item.is_a?(Hash) && item["type"] == "Create" ? item["object"] : item
    next unless object.is_a?(Hash)

    text = object["content"] || object["text"] || object["summary"] || object["name"]
    text = strip_html(text)
    next if text.empty?

    {
      action: "post_create",
      text: text,
      visibility: object["visibility"] || "followers",
      source_id: object["id"] || item["id"]
    }
  end.compact
end

actions = case options[:format]
when "opml" then opml_actions(options[:file])
when "rss-list" then rss_list_actions(options[:file])
when "mastodon-following-csv" then mastodon_following_actions(options[:file])
when "bluesky-follows", "bluesky-starter-pack" then bluesky_list_actions(options[:file])
when "local-posts-json", "mastodon-outbox-json" then local_posts_actions(options[:file])
else
  warn "Unknown --format #{options[:format]}"
  exit 2
end

deduped = []
seen = {}
actions.each do |action|
  key = [action[:action], action[:source_type], action[:watch_type], action[:url], action[:target], action[:text]].join("\0")
  next if seen[key]

  seen[key] = true
  command = command_for(action)
  deduped << action.merge(command: command)
end

plan = {
  format: "dais-import-plan-v1",
  created_at_utc: Time.now.utc.iso8601,
  source_format: options[:format],
  source_file: options[:file],
  instance_url: options[:instance_url],
  apply: options[:apply],
  action_count: deduped.length,
  actions: deduped
}

json = JSON.pretty_generate(plan)
if options[:output]
  File.write(options[:output], "#{json}\n")
else
  puts json
end

if options[:apply]
  token = options[:owner_token]
  if options[:owner_token_file]
    token = File.read(options[:owner_token_file]).strip
  end
  if token.to_s.empty?
    warn "--apply requires --owner-token, --owner-token-file, or DAIS_OWNER_TOKEN"
    exit 2
  end
  cli = options[:cli] ? Shellwords.split(options[:cli]) : ["cargo", "run", "--quiet", "--manifest-path", File.join(ROOT, "client/Cargo.toml"), "--"]
  env = {
    "DAIS_OWNER_TOKEN" => token,
    "DAIS_OWNER_INSTANCE_URL" => options[:instance_url]
  }
  deduped.each do |action|
    command = cli + action.fetch(:command)
    puts "APPLY #{action.fetch(:action)} #{(action[:target] || action[:url] || action[:source_id] || action[:text]).to_s[0, 120]}"
    unless system(env, *command)
      warn "Apply failed: #{command.shelljoin}"
      exit 1
    end
  end
end
