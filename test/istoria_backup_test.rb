# frozen_string_literal: true

require_relative 'istoria_test_case'

class IstoriaBackupTest < IstoriaTestCase
  def test_backs_up_lists_and_deletes_backups
    File.write(@history, "#1\nls\n#2\npwd\n")
    first, = back_up
    File.write(@history, "#2\npwd\n#3\ncd\n")
    second, counts = back_up

    assert_match(/\A\d{8}-\d{6}\z/, second)
    assert_equal '2 entries, 1 new', counts
    assert_equal 0o600, File.stat(File.join(data_home, 'istoria', 'history.db')).mode & 0o777
    assert_equal ["#{first}\t#{HOST}\t2\n#{second}\t#{HOST}\t2\n", '', 0], list_backups

    run_istoria('backup', 'delete', first)

    assert_equal ["#{second}\t#{HOST}\t2\n", '', 0], list_backups
  end

  def test_backs_up_the_history_of_another_host
    File.write(@history, "#1\nls\n")
    first, = back_up
    File.write(@history, "#1\nls\n#2\nssh mac\n")
    second, counts = back_up('--host', 'air.local')

    assert_equal '2 entries, 1 new', counts
    assert_equal ["#{first}\t#{HOST}\t1\n#{second}\tair\t2\n", '', 0], list_backups
  end

  def test_searches_every_backup_or_one
    File.write(@history, "echo secret\nls\n")
    first, = back_up
    File.write(@history, "pwd\n")
    back_up
    start('--backup')

    assert_equal ['backup›', *['pwd', 'ls', 'echo secret'].map { |command| backup_row(command) }], pane
    assert_color '38;5;3'

    @session.close
    start('--backup-id', first)

    assert_equal ["backup:#{first}›", backup_row('ls'), backup_row('echo secret')], pane
  end

  def test_shows_the_hosts_of_each_command
    File.write(@history, "ls\npwd\n")
    back_up('--host', 'mac')
    File.write(@history, "ls\ncd\n")
    back_up('--host', 'air')
    start('--backup')

    assert_equal ['backup›', backup_row('cd', 'air'), backup_row('pwd', 'mac'), backup_row('ls', 'air,mac')], pane
  end

  def test_deletes_from_every_backup_after_y
    File.write(@history, "echo secret\nls\n")
    first, = back_up
    second, = back_up
    start('--backup', 'secret')
    delete

    assert_match(/ delete from every backup\? y: echo secret ──\z/, @session.screen[DIVIDER_ROW])
    @session.type('y')
    @session.wait_for_text('deleted echo secret')
    assert_cancels(:escape)
    assert_equal ["#{first}\t#{HOST}\t1\n#{second}\t#{HOST}\t1\n", '', 0], list_backups
    assert_equal "echo secret\nls\n", File.read(@history)
  end

  def test_deletes_only_from_the_backup_searched
    File.write(@history, "echo secret\nls\n")
    first, = back_up
    second, = back_up
    start('--backup-id', first, 'secret')
    delete

    assert_match(/ delete from backup #{first}\? y: echo secret ──\z/, @session.screen[DIVIDER_ROW])
    @session.type('y')
    @session.wait_for_text('deleted echo secret')
    assert_cancels(:escape)
    assert_equal ["#{first}\t#{HOST}\t1\n#{second}\t#{HOST}\t2\n", '', 0], list_backups
  end

  def test_cancels_a_delete_from_backups_on_another_key
    File.write(@history, "echo secret\nls\n")
    back_up
    start('--backup', 'secret')
    delete
    @session.press(:escape)
    @session.wait_for { |lines| lines[DIVIDER_ROW].end_with?(' 1/2 ──') }

    assert_picks(:enter, 'echo secret')
    assert_match(/\t2\n\z/, list_backups.first)
  end

  def test_has_no_backups_before_the_first
    assert_equal ['', '', 0], list_backups
    _, stderr, status = run_istoria('search', '--backup-id', '20000101-000000')

    assert_equal 2, status.exitstatus
    assert_equal "istoria: no backup 20000101-000000\n", stderr
    refute File.exist?(data_home)
  end

  private

  def list_backups
    stdout, stderr, status = run_istoria('backup', 'list')

    [stdout, stderr, status.exitstatus]
  end
end
