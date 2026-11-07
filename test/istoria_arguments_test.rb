# frozen_string_literal: true

require_relative 'istoria_test_case'

class IstoriaArgumentsTest < IstoriaTestCase
  def test_prints_help
    stdout, stderr, status = run_istoria('--help')

    assert status.success?
    assert_empty stderr
    assert_match(/\AUsage: istoria COMMAND \[ARGS\]$/, stdout)
  end

  def test_prints_the_version
    version = File.read(File.join(CRATE, 'Cargo.toml'))[/^version = "(.+)"$/, 1]

    %w[-V --version].each do |option|
      stdout, stderr, status = run_istoria(option)

      assert status.success?
      assert_empty stderr
      assert_equal "istoria #{version}\n", stdout
    end
  end

  def test_help_and_version_fail_on_a_closed_stdout
    %w[--help --version].each do |option|
      stderr, status = run_istoria_into_a_closed_pipe(option)

      assert_equal 2, status.exitstatus
      assert_equal "istoria: cannot print: Broken pipe (os error 32)\n", stderr
    end
  end

  def test_rejects_unknown_option
    _, stderr, status = run_istoria('search', '-x')

    assert_equal 2, status.exitstatus
    assert_equal "istoria: invalid option: -x\nTry 'istoria --help' for usage.\n", stderr
  end

  def test_rejects_an_unknown_search_mode
    _, stderr, status = run_istoria('search', '--search', 'fuzzy')

    assert_equal 2, status.exitstatus
    assert_equal "istoria: invalid search mode: fuzzy\nTry 'istoria --help' for usage.\n", stderr
  end

  def test_rejects_an_option_given_twice
    _, stderr, status = run_istoria('search', '--prompt', 'a', '--prompt', 'b')

    assert_equal 2, status.exitstatus
    assert_equal "istoria: --prompt can only be given once\nTry 'istoria --help' for usage.\n", stderr
  end

  def test_joins_words_into_one_query
    start('sbx', 'run')

    assert_equal ['› sbx run', 'sbx run codex', 'sbx run claude (3)'], pane
  end

  def test_fails_on_a_missing_history_file
    _, stderr, status = run_istoria('search', 'ls', env: { 'HISTFILE' => File.join(@tmp_dir, 'missing') })

    assert_equal 2, status.exitstatus
    assert_match(/\Aistoria: cannot read .*missing: No such file or directory/, stderr)
  end
end
