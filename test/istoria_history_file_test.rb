# frozen_string_literal: true

require_relative 'istoria_test_case'

class IstoriaHistoryFileTest < IstoriaTestCase
  def test_keeps_other_lines_byte_for_byte
    content = "caf\xC3\xA9\nbad \xFF byte\n\nsbx run claude\nlast line without newline".b
    File.binwrite(@history, content)
    start('sbx')

    delete

    assert_equal "caf\xC3\xA9\nbad \xFF byte\n\nlast line without newline".b, File.binread(@history)
  end

  def test_deletes_timestamps_with_their_commands
    File.write(@history, "#1700000000\nls\n#1700000001\nfor x in a b\ndo echo $x\ndone\n#1700000002\npwd\n")
    start('for x')

    assert_equal ['› for x', '2023-11-14 22:13  for x in a b do echo $x done'], pane

    delete

    assert_equal "#1700000000\nls\n#1700000002\npwd\n", File.read(@history)
  end

  def test_keeps_lines_appended_while_open
    start('tmux')
    File.write(@history, "echo from another shell\n", mode: 'a')

    delete

    assert_equal "#{HISTORY.sub("tmux -CC\n", '')}echo from another shell\n", File.read(@history)
    assert_match(%r{ 0/15 ──\z}, @session.screen[DIVIDER_ROW].sub('deleted tmux -CC', '0/15'))
  end

  def test_refuses_to_write_a_file_rewritten_while_open
    start('')
    File.write(@history, "rewritten\n")

    @session.press(:ctrl_d)
    @session.wait_for_text('cannot delete')

    assert_includes @session.screen[DIVIDER_ROW], 'cannot delete: history file was changed by another process'
    assert_equal ['›', 'rewritten'], pane
    assert_equal "rewritten\n", File.read(@history)
  end

  def test_keeps_permissions
    File.chmod(0o600, @history)
    start('tmux')

    delete

    assert_equal 0o600, File.stat(@history).mode & 0o777
  end

  def test_rewrites_the_target_of_a_symlink
    link = File.join(@tmp_dir, 'link')
    File.symlink(@history, link)
    start('tmux', history: link)

    delete

    assert File.symlink?(link)
    refute_includes File.read(@history), 'tmux'
    assert_equal %w[bash_history link], Dir.children(@tmp_dir).sort
  end

  def test_falls_back_to_bash_history_in_home
    File.write(File.join(@tmp_dir, '.bash_history'), "ls\npwd\n")
    start('pw', env: { 'HISTFILE' => nil, 'HOME' => @tmp_dir })

    delete

    assert_equal "ls\n", File.read(File.join(@tmp_dir, '.bash_history'))
  end

  def test_delete_last_deletes_the_last_n_commands_only
    stdout, _, status = run_istoria('delete-last', '3')

    assert status.success?
    assert_equal "git push \nClaude\ngg\n", stdout
    assert_equal HISTORY.lines[0...-3].join, File.read(@history)
  end

  def test_delete_last_deletes_timestamps_with_their_commands
    File.write(@history, "#1\nls\n#2\nfor x\ndo\n")
    stdout, _, status = run_istoria('delete-last')

    assert status.success?
    assert_equal "for x\ndo\n", stdout
    assert_equal "#1\nls\n", File.read(@history)
  end

  def test_delete_last_deletes_everything_when_asked_for_more
    run_istoria('delete-last', '100')

    assert_empty File.read(@history)
  end

  def test_delete_last_leaves_an_empty_history_alone
    File.write(@history, "\n")
    inode = File.stat(@history).ino
    stdout, _, status = run_istoria('delete-last')

    assert status.success?
    assert_empty stdout
    assert_equal inode, File.stat(@history).ino
  end

  def test_delete_last_succeeds_when_the_reader_is_gone
    reader, writer = IO.pipe
    reader.close
    stderr = File.join(@tmp_dir, 'stderr')
    pid = Process.spawn({ 'HISTFILE' => @history }, ISTORIA, 'delete-last', '3', out: writer, err: stderr)
    writer.close
    _, status = Process.wait2(pid)

    assert status.success?
    assert_empty File.read(stderr)
    assert_equal HISTORY.lines[0...-3].join, File.read(@history)
  end
end
