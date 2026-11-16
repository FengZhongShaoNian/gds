"""Run real pytest subprocesses to guard against misleading acceptance reports."""

import json
from pathlib import Path
import xml.etree.ElementTree as ET

import pytest

pytest_plugins = ['pytester']
SUITE = Path(__file__).resolve().parents[1] / 'e2e'


@pytest.fixture
def runner(pytester):
    pytester.makeconftest(f'''
import importlib.util
import sys
sys.path.insert(0, {str(SUITE)!r})
spec = importlib.util.spec_from_file_location('gds_vm_plugin', {str(SUITE / 'conftest.py')!r})
plugin = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = plugin
spec.loader.exec_module(plugin)
pytest_plugins = ['gds_vm_plugin']
def unavailable(*args, **kwargs):
    raise plugin.Blocked('deliberately unavailable external VM')
plugin.Desktop = unavailable
''')
    return pytester


def test_blocked_environment_is_not_a_green_run(runner):
    runner.makepyfile('def test_desktop(desktop): pass')
    result = runner.runpytest_subprocess('--vm', 'ubuntu', '--artifacts', str(runner.path / 'evidence'))
    result.assert_outcomes(skipped=1)
    assert result.ret == pytest.ExitCode.TESTS_FAILED
    records = json.loads((runner.path / 'evidence/results.json').read_text())
    assert next(iter(records.values()))['status'] == 'blocked'
    assert ET.parse(runner.path / 'evidence/junit.xml').find('.//testcase/skipped') is not None


def test_teardown_failure_preserves_the_original_assertion(runner):
    runner.makepyfile('''
import pytest
@pytest.fixture
def resource():
    yield
    raise RuntimeError('cleanup failure')
def test_workflow(resource):
    assert False, 'original product assertion'
''')
    result = runner.runpytest_subprocess('--vm', 'ubuntu', '--artifacts', str(runner.path / 'evidence'))
    result.assert_outcomes(failed=1, errors=1)
    record = next(iter(json.loads((runner.path / 'evidence/results.json').read_text()).values()))
    assert record['status'] == 'failed'
    assert 'original product assertion' in record['error']
    assert len(record['failures']) == 2
    assert 'cleanup failure' in record['failures'][1]['error']


def test_collection_does_not_claim_execution(runner):
    runner.makepyfile('def test_workflow(desktop): pass')
    result = runner.runpytest_subprocess('--vm', 'ubuntu', '--collect-only',
                                       '--artifacts', str(runner.path / 'evidence'))
    assert result.ret == pytest.ExitCode.OK
    record = next(iter(json.loads((runner.path / 'evidence/results.json').read_text()).values()))
    assert record['status'] == 'planned'
    assert record['seconds'] == 0


def test_termination_during_cleanup_keeps_case_unfinished(runner):
    runner.makepyfile('''
import os
import signal
import pytest
@pytest.fixture
def resource():
    yield
    os.kill(os.getpid(), signal.SIGTERM)
def test_workflow(resource):
    assert True
''')
    result = runner.runpytest_subprocess('--vm', 'ubuntu', '--artifacts', str(runner.path / 'evidence'))
    assert result.ret != pytest.ExitCode.OK
    record = next(iter(json.loads((runner.path / 'evidence/results.json').read_text()).values()))
    assert record['call_passed']
    assert record['status'] == 'planned'
