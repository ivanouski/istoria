# frozen_string_literal: true

require_relative 'test_helper'
require_relative 'terminal_session'

class IstoriaTestCase < Minitest::Test
  CRATE = ROOT
  ISTORIA_DIR = File.join(CRATE, 'target', 'debug')
  ISTORIA = File.join(ISTORIA_DIR, 'istoria')
  ISTORIARC = File.join(ROOT, 'home', '.istoriarc')

  HISTORY = [
    'cd src/sndbx/',
    'git fetch --all --prune',
    'sbx run claude ',
    'tmux -CC',
    'sbx -V',
    'sbx run claude ',
    'gg',
    'ga .',
    "git cm 'Align the recalculation status'",
    'sbx build claude ',
    'sbx run claude',
    'tc',
    'sbx run codex ',
    './install.rb ',
    'claude --resume',
    'git push ',
    'Claude',
    'gg',
  ].map { |line| "#{line}\n" }.join.freeze

  system('cargo', 'build', '--quiet', '--manifest-path', File.join(CRATE, 'Cargo.toml')) || abort('cargo build failed')

  def setup
    @tmp_dir = Dir.mktmpdir
    @history = File.join(@tmp_dir, 'bash_history')
    File.write(@history, HISTORY)
  end

  def teardown
    @session&.close
    FileUtils.remove_entry(@tmp_dir)
  end

  DIVIDER_ROW = 1
  FIRST_ROW = 2
  PANE_DIVIDER = %r{\A─* ?(\d+/\d+|deleted .*|cannot .*|no delete .*) ──\z}

  private

  def start(*query, command: [ISTORIA, 'search', *query], history: @history, env: {}, **terminal)
    env = istoria_env(HISTFILE: history).merge(env)
    @session = TerminalSession.new(command, env:, **terminal)
    wait_for_pane
  end

  def bash_with_istoriarc(commands)
    ['bash', '--norc', '--noprofile', '-i', '-c', ". #{ISTORIARC}; #{commands}"]
  end

  def start_bash(path: ISTORIA_DIR, ps1: '$ ', bashrc: '')
    rc = File.join(@tmp_dir, 'bashrc')
    File.write(rc, <<~BASH)
      #{bashrc}
      PS1='#{ps1}'
      HISTFILE=#{@history}
      PROMPT_COMMAND='history -a; history -c; history -r'
      . #{ISTORIARC}
    BASH
    env = {
      'PATH' => "#{path}:#{ENV.fetch('PATH')}", 'HOME' => @tmp_dir, 'XDG_CONFIG_HOME' => nil, 'XDG_DATA_HOME' => nil,
      'INPUTRC' => File::NULL,
      'TZ' => 'UTC', 'ISTORIA_SHELL_PROMPT' => nil, 'ISTORIA_PROMPT' => nil,
    }
    @session = TerminalSession.new(['bash', '--noprofile', '--rcfile', rc, '-i'], env:)
    @session.wait_for { |lines| !lines.first.empty? }
  end

  def wait_for_istoria_in_bash(query = '', prompt: '$')
    prompt = "#{prompt} #{query}".rstrip
    @session.wait_for { |lines| lines.each_cons(2).any? { |line, below| line == prompt && below.match?(PANE_DIVIDER) } }
  end

  # istoria draws bash's prompt line too, only its pane going tells them apart
  def wait_for_bash(line)
    @session.wait_for { |lines| lines.first == line && lines.none? { |below| below.match?(PANE_DIVIDER) } }
  end

  def wait_for_pane
    @session.wait_for { |lines| lines.any? { |line| line.match?(PANE_DIVIDER) } }
  end

  def pane
    lines = @session.screen
    divider = lines.index { |line| line.match?(PANE_DIVIDER) }
    flunk("no pane under a prompt line:\n#{lines.join("\n")}") unless divider&.positive?

    [lines[divider - 1], *lines[(divider + 1)..].take_while { |line| !line.empty? }.map(&:strip)]
  end

  def picked_output(*keys)
    output = File.join(@tmp_dir, 'output')
    start(command: ['sh', '-c', %("$0" search > "#{output}"), ISTORIA])
    @session.press(*keys)
    @session.wait
    @session = nil

    File.read(output)
  end

  def assert_history(expected)
    @session.wait_for { |lines| lines.grep(/\A +\d/).size >= expected.size }

    assert_equal expected, @session.screen.grep(/\A +\d/)
  end

  # An earlier status may still show
  def delete
    before = @session.screen
    @session.press(:ctrl_d)
    @session.wait_for { |lines| lines != before && lines.any? { |line| line.match?(/ (deleted|cannot|no delete) /) } }
  end

  def assert_picks(*keys, command, status: 0)
    @session.press(*keys)

    assert_equal status, @session.wait.exitstatus, @session.screen.join("\n")
    assert_equal command, @session.screen.join("\n").strip
  end

  def assert_cancels(*keys, status: 1, output: '')
    @session.press(*keys)

    assert_equal status, @session.wait.exitstatus
    assert_equal output, @session.screen.join("\n").strip
  end

  def run_istoria(*, env: {})
    Open3.capture3(istoria_env.merge(env), ISTORIA, *)
  end

  # Backs up the history, returns the label and the counts. A backup waits for the next second, as
  # two in one second would have the same label.
  def back_up
    sleep(0.05) until Time.now.to_i > @backed_up_at.to_i
    stdout, stderr, status = run_istoria('backup')
    @backed_up_at = Time.now.to_i

    assert status.success?, stderr
    stdout.chomp.split('  ')
  end

  # Runs istoria with stdout on a pipe nobody reads, so every write fails with EPIPE
  def run_istoria_into_a_closed_pipe(*)
    out_reader, out_writer = IO.pipe
    out_reader.close
    err_reader, err_writer = IO.pipe
    pid = Process.spawn(istoria_env, ISTORIA, *,
                        out: out_writer, err: err_writer)
    [out_writer, err_writer].each(&:close)
    stderr = err_reader.read
    err_reader.close

    [stderr, Process.wait2(pid).last]
  end

  def assert_color(code)
    assert_match(color_pattern(code), @session.output)
  end

  def refute_color(code)
    refute_match(color_pattern(code), @session.output)
  end

  def color_pattern(code)
    /\e\[(?:[\d;]*;)?#{Regexp.escape(code)}[;m]/
  end

  # The history, the config and backups of the test's own. One time zone, as backup labels are in
  # local time.
  def istoria_env(**env)
    { 'HISTFILE' => @history, 'XDG_CONFIG_HOME' => config_home, 'XDG_DATA_HOME' => data_home, 'TZ' => 'UTC' }
      .merge(env.transform_keys(&:to_s))
  end

  def config_home
    File.join(@tmp_dir, 'config')
  end

  def data_home
    File.join(@tmp_dir, 'data')
  end

  def write_config(text, home: config_home)
    FileUtils.mkdir_p(File.join(home, 'istoria'))
    File.write(File.join(home, 'istoria', 'config.toml'), text)
  end
end
