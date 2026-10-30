# frozen_string_literal: true

require 'socket'
require_relative 'istoria_test_case'

class IstoriaBackupTest < IstoriaTestCase
  HOST = Socket.gethostname.split('.').first

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

  def test_searches_every_backup_or_one
    File.write(@history, "echo secret\nls\n")
    first, = back_up
    File.write(@history, "pwd\n")
    back_up
    start('--backup')

    assert_equal ['backup›', 'pwd', 'ls', 'echo secret'], pane
    assert_color '38;5;5'

    @session.close
    start('--backup-id', first)

    assert_equal ["backup:#{first}›", 'ls', 'echo secret'], pane
  end

  def test_picks_but_deletes_nothing_from_backups
    File.write(@history, "echo secret\nls\n")
    back_up
    start('--backup', 'secret')
    delete

    assert_match(/ no delete available for backups ──\z/, @session.screen[DIVIDER_ROW])
    assert_picks(:enter, 'echo secret')
    assert_equal "echo secret\nls\n", File.read(@history)
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
