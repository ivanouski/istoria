# frozen_string_literal: true

require_relative 'istoria_test_case'

class IstoriaConfigTest < IstoriaTestCase
  def test_takes_colors_from_the_config
    write_config(<<~TOML)
      [colors]
      match = "#ff8800"
      selected = { bg = "21" }
    TOML
    start('cd')

    assert_color '38;2;255;136;0'
    assert_color '48;5;21'
    refute_color '48;5;8'
  end

  def test_takes_the_pane_height_from_the_config
    write_config("height = 6\n")
    start('')

    # The prompt line and the commands under the divider
    assert_equal 5, pane.size
  end

  def test_fits_a_pane_higher_than_the_terminal
    write_config("height = 100\n")
    start('', rows: 8)

    assert_equal 7, pane.size
    assert_picks(:enter, 'gg')
  end

  def test_falls_back_to_the_config_in_home
    write_config(%([colors]\nselected = { bg = "21" }\n), home: File.join(@tmp_dir, '.config'))
    start('cd', env: { 'XDG_CONFIG_HOME' => nil, 'HOME' => @tmp_dir })

    assert_color '48;5;21'
  end

  def test_fails_on_a_broken_config
    write_config(%([colors]\nmatch = "yelow"\n))
    _, stderr, status = run_istoria('search', 'ls')

    assert_equal 2, status.exitstatus
    assert_match(%r{\Aistoria: #{Regexp.escape(config_home)}/istoria/config.toml: TOML parse error at line 2}, stderr)
    assert_includes stderr, 'unknown color "yelow"'
  end

  def test_init_config_writes_the_defaults_from_help
    path = File.join(config_home, 'istoria', 'config.toml')
    stdout, stderr, status = run_istoria('init-config')
    help, = run_istoria('--help')
    defaults = help[/^    height.*\n(?:    .*\n)+/].gsub(/^    /, '')

    assert status.success?, stderr
    assert_equal "#{path}\n", stdout
    assert_equal defaults, File.read(path)
  end

  def test_init_config_keeps_an_existing_config
    write_config('broken')
    path = File.join(config_home, 'istoria', 'config.toml')
    stdout, stderr, status = run_istoria('init-config')

    assert_equal 2, status.exitstatus
    assert_empty stdout
    assert_equal "istoria: #{path} already exists\n", stderr
    assert_equal 'broken', File.read(path)
  end

  def test_init_config_ignores_a_relative_config_home
    stdout, _, status = Open3.capture3({ 'HISTFILE' => @history, 'XDG_CONFIG_HOME' => 'relative', 'HOME' => @tmp_dir },
                                       ISTORIA, 'init-config', chdir: @tmp_dir)

    assert status.success?
    assert_equal "#{File.join(@tmp_dir, '.config', 'istoria', 'config.toml')}\n", stdout
    refute File.exist?(File.join(@tmp_dir, 'relative'))
  end

  def test_init_config_leaves_no_half_written_config
    path = File.join(config_home, 'istoria', 'config.toml')
    _, stderr, status = Open3.capture3(
      { 'XDG_CONFIG_HOME' => config_home }, 'sh', '-c', %(trap '' XFSZ; ulimit -f 0; exec "$0" init-config), ISTORIA
    )

    assert_equal 2, status.exitstatus
    assert_match(/\Aistoria: cannot create #{Regexp.escape(path)}: File too large/, stderr)
    refute File.exist?(path)
  end

  def test_init_config_fails_on_a_closed_stdout
    stderr, status = run_istoria_into_a_closed_pipe('init-config')

    assert_equal 2, status.exitstatus
    assert_equal "istoria: cannot print: Broken pipe (os error 32)\n", stderr
    assert File.exist?(File.join(config_home, 'istoria', 'config.toml'))
  end

  def test_starts_with_the_written_config
    run_istoria('init-config')
    start('cd')

    assert_color '48;5;8'
  end
end
