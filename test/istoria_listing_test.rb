# frozen_string_literal: true

require_relative 'istoria_test_case'

class IstoriaListingTest < IstoriaTestCase
  LONG_COMMAND = 'kubectl apply -f k8s/prod/ --context=prod-eu-west-1 --server-side ' \
                 '--force-conflicts && ./deploy.sh notify'

  def test_lists_matches_most_recent_first
    start('clau')

    assert_equal ['› clau', 'Claude', 'claude --resume', 'sbx run claude (3)', 'sbx build claude'], pane
  end

  def test_prefix_search_matches_the_start_of_commands
    write_config(%(search = "prefix"\n))
    start('clau')

    assert_equal ['› clau', 'Claude', 'claude --resume'], pane
  end

  def test_search_option_matches_the_start_of_commands
    start('--search', 'prefix', 'clau')

    assert_equal ['› clau', 'Claude', 'claude --resume'], pane
  end

  def test_search_option_overrides_the_config
    write_config(%(search = "prefix"\n))
    start('--search', 'keyword', 'clau')

    assert_equal ['› clau', 'Claude', 'claude --resume', 'sbx run claude (3)', 'sbx build claude'], pane
  end

  def test_lists_matches_with_a_typo_after_the_others
    File.write(@history, "git push\ngti status\ngit log\n")
    start('gti')

    assert_equal ['› gti', 'gti status', 'git log', 'git push'], pane
  end

  def test_typos_can_be_turned_off
    write_config("typos = false\n")
    start('gti')

    assert_equal ['› gti', 'no matches'], pane
  end

  def test_no_typos_option_turns_them_off
    start('--no-typos', 'gti')

    assert_equal ['› gti', 'no matches'], pane
  end

  def test_typos_option_overrides_the_config
    File.write(@history, "git push\ngti status\ngit log\n")
    write_config("typos = false\n")
    start('--typos', 'gti')

    assert_equal ['› gti', 'gti status', 'git log', 'git push'], pane
  end

  def test_typing_narrows_the_list
    start('clau')
    @session.type('de --')
    @session.wait_for_text('› claude --')

    assert_equal ['› claude --', 'claude --resume'], pane
  end

  def test_edits_the_query_at_the_cursor
    start('sbx rn')
    @session.press(:left)
    @session.type('u')
    @session.wait_for_text('› sbx run')

    assert_equal ['› sbx run', 'sbx run codex', 'sbx run claude (3)'], pane
    @session.wait_for { @session.cursor == [0, '› sbx ru'.size] }

    @session.press(:home, :delete, :end)
    @session.type('!')

    @session.wait_for_text('› bx run!')
  end

  def test_shows_the_match_of_a_long_command_and_previews_it
    File.write(@history, "#{LONG_COMMAND}\nls\n")
    write_config("preview = true\n")
    start('deploy')

    assert_equal "  …#{LONG_COMMAND[-77..]}", @session.screen[FIRST_ROW]

    divider = @session.screen.index('─' * 80)

    assert_equal LONG_COMMAND, @session.screen[(divider + 1)..].take_while { |line| !line.empty? }.join(' ')
  end

  def test_ignores_letters_typed_with_alt
    start('clau')

    @session.type("\ex")
    @session.type('d')
    @session.wait_for { |lines| lines.first == '› claud' }

    assert_equal ['› claud', 'Claude', 'claude --resume', 'sbx run claude (3)', 'sbx build claude'], pane
  end
end
