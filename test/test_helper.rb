# frozen_string_literal: true

require 'minitest/autorun'
require 'fileutils'
require 'json'
require 'open3'
require 'time'
require 'tmpdir'

# Each test has its own directory and terminal
Minitest::Test.parallelize_me!

ROOT = File.expand_path('..', __dir__)
