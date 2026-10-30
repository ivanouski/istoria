# frozen_string_literal: true

require_relative 'istoria_test_case'

class IstoriaCompletionTest < IstoriaTestCase
  COMPLETION = File.join(ROOT, 'completions', '_istoria')

  COMPLETE_LAST_WORD = <<~'BASH'
    source "$1"
    shift
    COMP_WORDS=("$@")
    COMP_CWORD=$(( $# - 1 ))
    _istoria
    printf '%s\n' "${COMPREPLY[@]}"
  BASH

  def test_offers_the_commands_first
    assert_equal %w[search delete-last init-config backup], complete('')
    assert_equal %w[delete-last], complete('d')
    assert_equal %w[backup], complete('b')
  end

  def test_offers_help_and_version_first
    assert_equal %w[--help --version], complete('--')
    assert_equal %w[-h -V], complete('-')
  end

  def test_dash_dash_offers_every_search_option
    assert_equal %w[--print-query --prompt --backup --backup-id], complete('search', '--')
  end

  def test_completes_a_partial_search_option
    assert_equal %w[--print-query --prompt], complete('search', '--pr')
    assert_equal %w[--prompt], complete('search', '--pro')
    assert_equal %w[--backup --backup-id], complete('search', '--b')
  end

  def test_query_words_are_not_completed
    assert_empty complete('search', '')
    assert_empty complete('search', 'git', '')
    assert_empty complete('search', 'g')
  end

  def test_offers_search_options_after_the_query
    assert_equal %w[--print-query --prompt --backup --backup-id], complete('search', 'git', '--')
  end

  def test_search_options_are_not_offered_again
    assert_equal %w[--prompt --backup --backup-id], complete('search', '--print-query', '--')
    assert_equal %w[--print-query], complete('search', '--prompt', '$ ', '--')
  end

  def test_backup_options_go_apart_from_each_other_and_the_prompt
    assert_equal %w[--print-query], complete('search', '--backup', '--')
    assert_equal %w[--print-query], complete('search', '--backup-id', '20261030-142501', '--')
  end

  def test_prompt_text_is_not_completed
    assert_empty complete('search', '--prompt', '--')
    assert_empty complete('search', '--prompt', '-')
  end

  def test_a_prompt_text_like_an_option_does_not_stop_completion
    assert_equal %w[--print-query], complete('search', '--prompt', '--help', '--')
  end

  def test_nothing_follows_options_that_exit
    assert_empty complete('--help', '')
    assert_empty complete('-V', '-')
    assert_empty complete('search', '--help', '--')
    assert_empty complete('search', '-V', '-')
  end

  def test_nothing_follows_commands_without_options
    assert_empty complete('init-config', '')
    assert_empty complete('delete-last', '')
    assert_empty complete('delete-last', '3', '')
  end

  def test_nothing_follows_double_dash
    assert_empty complete('search', '--', '--')
    assert_empty complete('search', '--', '-')
  end

  def test_offers_the_backup_commands
    assert_equal %w[list delete], complete('backup', '')
    assert_equal %w[list], complete('backup', 'l')
    assert_empty complete('backup', 'list', '')
  end

  def test_offers_the_labels_of_the_backups
    assert_empty complete('backup', 'delete', '')

    label, = back_up

    assert_equal [label], complete('backup', 'delete', '')
    assert_equal [label], complete('search', 'git', '--backup-id', label[0, 4])
    assert_empty complete('backup', 'delete', label, '')
  end

  private

  def complete(*words)
    env = istoria_env(PATH: "#{ISTORIA_DIR}:#{ENV.fetch('PATH')}")
    stdout, stderr, status = Open3.capture3(env, 'bash', '-c', COMPLETE_LAST_WORD, 'bash', COMPLETION, 'istoria',
                                            *words)
    assert status.success?, stderr

    stdout.lines(chomp: true).reject(&:empty?)
  end
end
