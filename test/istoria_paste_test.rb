# frozen_string_literal: true

require_relative 'istoria_test_case'

class IstoriaPasteTest < IstoriaTestCase
  ENABLE = "\e[?2004h"
  DISABLE = "\e[?2004l"

  def test_paste_goes_in_the_query_and_its_newlines_dont_pick
    start('--print-query', 'sbx')

    @session.paste("\r\nclaude\trun\n")
    @session.wait_for { |lines| lines.first == '› sbx claude run' }

    assert_equal ['› sbx claude run', 'sbx run claude (3)'], pane
    assert_cancels(:escape, status: 3, output: 'sbx claude run')
  end

  def test_turns_bracketed_paste_on_while_open_when_run_directly
    start

    assert_includes @session.output, ENABLE
    refute_includes @session.output, DISABLE

    output = @session.output
    @session.press(:escape)
    @session.wait

    assert_includes @session.output.delete_prefix(output), DISABLE
  end

  def test_leaves_bracketed_paste_to_readline
    File.write(@history, "echo pasted here\nls\n")
    start_bash

    before = @session.output.length
    @session.press(:up)
    wait_for_istoria_in_bash
    @session.paste("pasted\nhere\n")
    wait_for_istoria_in_bash('pasted here')

    assert_equal ['$ pasted here', 'echo pasted here'], pane

    @session.press(:escape)
    wait_for_bash('$ pasted here')

    refute_match(/\e\[\?2004[hl]/, @session.output[before..])
  end
end
