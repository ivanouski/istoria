# frozen_string_literal: true

require_relative 'istoria_test_case'

class IstoriarcTest < IstoriaTestCase
  def test_istoriarc_binds_up_and_down
    stdout, = Open3.capture3({ 'PATH' => "#{ISTORIA_DIR}:#{ENV.fetch('PATH')}" },
                             *bash_with_istoriarc('bind -X; bind -s'))

    bindings = stdout.lines(chomp: true).map { |line| line.scan(/"([^"]*)"/).flatten }
    macro = '\e[9100~\e[9101~'
    backups = '\e[9102~\e[9101~'

    expected = [
      ['\eOA', macro], ['\eOB', backups], ['\e[9100~', '__istoria_search'], ['\e[9102~', '__istoria_search --backup'],
      ['\e[A', macro], ['\e[B', backups],
    ]

    assert_equal expected, bindings.sort
  end

  def test_istoriarc_does_nothing_in_a_non_interactive_shell
    stdout, stderr, status = Open3.capture3({ 'PATH' => "#{ISTORIA_DIR}:#{ENV.fetch('PATH')}" },
                                            'bash', '--norc', '--noprofile', '-c',
                                            ". #{ISTORIARC}; type __istoria_search")

    refute status.success?
    assert_empty stdout
    assert_equal ["bash: line 1: type: __istoria_search: not found\n"], [stderr]
  end

  def test_istoriarc_does_nothing_without_istoria
    stdout, stderr, status = Open3.capture3({ 'PATH' => '/usr/bin:/bin' },
                                            *bash_with_istoriarc('bind -X; type __istoria_search'))

    refute status.success?
    assert_empty stdout
    assert_includes stderr, '__istoria_search: not found'
  end

  def test_up_opens_istoria_with_the_typed_text_and_enter_runs_the_command
    File.write(@history, "echo first\necho picked\nls\n")
    start_bash

    @session.type('pick')
    @session.press(:up)
    wait_for_istoria_in_bash('pick')

    assert_equal ['$ pick', 'echo picked'], pane

    @session.press(:enter)

    @session.wait_for { |lines| lines.first(3) == ['$ echo picked', 'picked', '$'] }
  end

  def test_down_searches_the_backups_for_the_typed_text_and_enter_runs_the_command
    File.write(@history, "echo backed up\nls\n")
    run_istoria('backup', env: { 'XDG_DATA_HOME' => nil, 'HOME' => @tmp_dir })
    File.write(@history, "echo first\n")
    start_bash

    @session.type('echo')
    @session.press(:down)
    wait_for_istoria_in_bash('echo', prompt: '$ backup›')

    assert_equal ['$ backup› echo', 'echo backed up'], pane

    @session.press(:enter)

    @session.wait_for { |lines| lines.first(3) == ['$ echo backed up', 'backed up', '$'] }
  end

  def test_tab_puts_the_command_on_the_line_without_running_it
    File.write(@history, "echo first\necho picked\n")
    start_bash

    @session.press(:up)
    wait_for_istoria_in_bash
    @session.press(:tab)
    @session.wait_for { |lines| lines.first == '$ echo picked' }
    @session.type('X')

    @session.wait_for { |lines| lines.first == '$ echo pickedX' }
    assert_equal '', @session.screen[1]
  end

  def test_enter_runs_the_command_into_history
    File.write(@history, "echo first\necho second\n")
    start_bash

    @session.press(:up)
    wait_for_istoria_in_bash
    @session.press(:down, :enter)
    @session.wait_for { |lines| lines.include?('first') }
    @session.type("history\r")

    assert_history ['    1  echo first', '    2  echo second', '    3  echo first', '    4  history']
  end

  def test_up_on_an_empty_line_lists_every_command
    File.write(@history, "echo first\nls\n")
    start_bash

    @session.press(:up)
    wait_for_istoria_in_bash

    assert_equal ['$', 'ls', 'echo first'], pane
  end

  def test_shows_the_shell_prompt_in_its_colors
    File.write(@history, "echo first\necho picked\n")
    start_bash(ps1: '\[\e]0;title\a\e[35m\]box\[\e[0m\]\$ ')

    @session.type('pick')
    @session.press(:up)
    @session.wait_for { |lines| lines.first == 'box$ pick' && lines[DIVIDER_ROW].end_with?(' 1/2 ──') }

    assert_color '38;5;5'
    refute_includes @session.output.partition("\e[6n").last, 'title'
  end

  def test_lists_commands_under_the_query_after_a_long_prompt
    File.write(@history, "#1700000000\necho first\n#1700000000\necho picked\n")
    start_bash(ps1: '[user@pc ~/src/utils (main)]\$ ')

    @session.type('echo')
    @session.press(:up)
    @session.wait_for do |lines|
      lines.first == '[user@pc ~/src/utils (main)]$ echo' && lines[DIVIDER_ROW].end_with?(' 2/2 ──')
    end

    assert_equal "#{' ' * 12}2023-11-14 22:13  echo picked", @session.screen[FIRST_ROW]
  end

  def test_shows_the_simple_prompt_without_expanding_the_shell_prompt_when_turned_off
    File.write(@history, "echo first\necho picked\n")
    # Bash expands PS1 once for its own prompt, every expansion adds a line
    expansions = File.join(@tmp_dir, 'expansions')
    start_bash(ps1: "$(echo >> #{expansions})box\\$ ", bashrc: 'ISTORIA_SHELL_PROMPT=false')

    @session.type('pick')
    @session.press(:up)
    @session.wait_for { |lines| lines.first == '› pick' && lines[DIVIDER_ROW].end_with?(' 1/2 ──') }

    assert_equal 1, File.readlines(expansions).size
  end

  def test_shows_istoria_prompt_in_its_colors_without_expanding_the_shell_prompt
    File.write(@history, "echo first\necho picked\n")
    expansions = File.join(@tmp_dir, 'expansions')
    start_bash(ps1: "$(echo >> #{expansions})box\\$ ", bashrc: %q(ISTORIA_PROMPT='\[\e[32m\]hist›\[\e[0m\] '))

    @session.type('pick')
    @session.press(:up)
    @session.wait_for { |lines| lines.first == 'hist› pick' && lines[DIVIDER_ROW].end_with?(' 1/2 ──') }

    assert_color '38;5;2'
    assert_equal 1, File.readlines(expansions).size
  end

  def test_up_opens_istoria_with_nounset_on
    File.write(@history, "echo first\nls\n")
    start_bash(bashrc: 'set -u')

    @session.press(:up)
    wait_for_istoria_in_bash

    assert_equal ['$', 'ls', 'echo first'], pane
  end

  def test_ctrl_p_and_ctrl_n_browse_history_as_usual
    File.write(@history, "echo first\necho second\n")
    start_bash

    @session.press(:ctrl_p)
    @session.wait_for { |lines| lines.first == '$ echo second' }
    @session.press(:ctrl_p)
    @session.wait_for { |lines| lines.first == '$ echo first' }
    @session.press(:ctrl_n)

    @session.wait_for { |lines| lines.first == '$ echo second' }
  end

  def test_escape_keeps_the_typed_text
    start_bash

    @session.type('sbx')
    @session.press(:up)
    wait_for_istoria_in_bash('sbx')
    @session.press(:escape)

    @session.wait_for { |lines| lines.first == '$ sbx' }
  end

  def test_escape_puts_the_edited_query_on_the_line
    start_bash

    @session.type('sbx run')
    @session.press(:up)
    wait_for_istoria_in_bash('sbx run')
    @session.press(:backspace, :backspace, :backspace, :backspace)
    wait_for_istoria_in_bash('sbx')
    @session.press(:escape)
    wait_for_bash('$ sbx')
    @session.type('X')

    @session.wait_for { |lines| lines.first == '$ sbxX' }
  end

  def test_escape_without_edits_keeps_the_cursor
    start_bash

    @session.type("sbx run\e[D\e[D\e[D")
    @session.press(:up)
    wait_for_istoria_in_bash('sbx run')
    @session.press(:escape)
    wait_for_bash('$ sbx run')
    @session.type('X')

    @session.wait_for { |lines| lines.first == '$ sbx Xrun' }
  end

  def test_escape_keeps_trailing_spaces_of_the_query
    start_bash

    @session.type('sbx run')
    @session.press(:up)
    wait_for_istoria_in_bash('sbx run')
    @session.press(:backspace, :backspace, :backspace)
    wait_for_istoria_in_bash('sbx')
    @session.press(:escape)
    wait_for_bash('$ sbx')
    @session.type('X')

    @session.wait_for { |lines| lines.first == '$ sbx X' }
  end

  def test_enter_without_matches_puts_the_query_on_the_line
    start_bash

    @session.type('sbx run')
    @session.press(:up)
    wait_for_istoria_in_bash('sbx run')
    @session.type('zzz')
    wait_for_istoria_in_bash('sbx runzzz')
    @session.press(:enter)

    @session.wait_for { |lines| lines.first == '$ sbx runzzz' }
  end

  def test_an_older_istoria_leaves_the_typed_text
    old_bin = File.join(@tmp_dir, 'old_bin')
    FileUtils.mkdir_p(old_bin)
    File.write(File.join(old_bin, 'istoria'), <<~SH)
      #!/bin/sh
      echo "istoria: invalid option: --print-query" >&2
      exit 1
    SH
    File.chmod(0o755, File.join(old_bin, 'istoria'))
    start_bash(path: old_bin)

    @session.type('sbx run')
    @session.press(:up)

    @session.wait_for_text('istoria: invalid option: --print-query')
    @session.wait_for { |lines| lines.any? { |line| line == '$ sbx run' } }
  end

  def test_ctrl_c_restores_the_typed_text
    start_bash

    @session.type('sbx run')
    @session.press(:up)
    wait_for_istoria_in_bash('sbx run')
    @session.press(:backspace, :backspace, :backspace, :backspace)
    @session.wait_for { |lines| !lines.first.include?('run') }
    @session.press(:ctrl_c)

    @session.wait_for { |lines| lines.first == '$ sbx run' }
  end

  def test_an_error_leaves_the_typed_text
    start_bash
    @session.type("HISTFILE=#{@tmp_dir}\r")
    @session.wait_for { |lines| lines[1] == '$' }

    @session.type('sbx run')
    @session.press(:up)

    @session.wait_for_text('istoria: cannot read')
    @session.wait_for { |lines| lines.last(24).any? { |line| line == '$ sbx run' } }
  end

  def test_finds_commands_from_the_current_shell
    File.write(@history, '')
    start_bash

    @session.type("echo fresh\r")
    @session.wait_for_text('fresh')
    @session.press(:up)
    wait_for_istoria_in_bash

    assert_equal ['$', 'echo fresh'], pane
  end

  def test_deleted_commands_leave_the_shell_history_at_the_next_prompt
    File.write(@history, "echo secret\necho keep\n")
    start_bash

    @session.press(:up)
    wait_for_istoria_in_bash
    @session.press(:down)
    delete
    @session.press(:escape)
    wait_for_bash('$')
    @session.type("\rhistory\r")
    @session.wait_for { |lines| lines.first(5).last == '$' }

    assert_equal ['$', '$ history', '    1  echo keep', '    2  history', '$'], @session.screen.first(5)
    assert_equal "echo keep\nhistory\n", File.read(@history)
  end

  def test_keeps_the_shell_history_intact
    File.write(@history, "echo first\necho second\n")
    start_bash

    @session.press(:up)
    wait_for_istoria_in_bash
    @session.press(:tab)
    @session.wait_for_text('$ echo second')
    @session.press(:ctrl_u)
    @session.type("history\r")

    assert_history ['    1  echo first', '    2  echo second', '    3  history']
  end
end
