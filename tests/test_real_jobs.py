"""The real-job transcripts, their v0.94 excerpts and contract/serial/real-jobs.md stay in step."""
import json
import sys

import serial_contract

sys.path.insert(0, str(serial_contract.REPO / "tools"))

import compare_real_jobs  # noqa: E402

REAL = serial_contract.CONTRACT / "gcode" / "real"


def names():
    return sorted(path.stem for path in (serial_contract.CONTRACT / "real-jobs").glob("*.json"))


def test_every_real_job_has_its_gcode_and_transcript():
    assert len(names()) >= 6
    for name in names():
        assert (REAL / (name + ".nc")).is_file()
        assert (serial_contract.CONTRACT / "transcripts" / (name + ".json")).is_file()


def test_the_owners_gcode_stays_trimmed():
    assert sum(path.stat().st_size for path in REAL.iterdir()) < 2_000_000


def test_no_real_job_commits_a_whole_file():
    for name in names():
        job = json.loads((serial_contract.CONTRACT / "real-jobs" / (name + ".json")).read_text())
        kept = sum(end - start for start, end in job["segments"])
        assert kept < job["fileLines"], name
        assert len((REAL / (name + ".nc")).read_text().splitlines()) == kept, name


def test_the_report_is_current():
    expected = compare_real_jobs.report([compare_real_jobs.compare(name) for name in names()])
    assert compare_real_jobs.REPORT.read_text() == expected, "run tools/compare_real_jobs.py"


def test_nothing_the_backend_sends_for_a_real_job_is_unexplained():
    assert [r["unexplained"] for r in map(compare_real_jobs.compare, names()) if r["unexplained"]] == []


def test_only_x_and_y_digits_count_as_home_decimals():
    home = compare_real_jobs.HOME_DECIMALS
    assert compare_real_jobs.classify_pair("G1 X1.0000000001 Y2  \n", "G1 X1.0 Y2  \n") == home
    for v094, backend in [("G1 X1.0 Y2  \n", "G1 X1 Y2 \n"), ("G1 X1.0 Y2 (keep)\n", "G1 X1 Y2\n"),
                          ("G1 X1.0 Y2 F100\n", "G1 X1 Y2 F100.0\n"), ("G1 X1.0 Y2\n", "G1 X1.001 Y2\n")]:
        assert compare_real_jobs.classify_pair(v094, backend) is None, (v094, backend)


def test_a_resume_sequence_explains_only_the_lines_it_is_made_of():
    tool, pause = {"resumeAfterToolChange": True}, {"resumeAfterPause": True}
    resume = ["~\n", "G21  \n", "G90  \n", "G0 Z5.0  \n", "G0 X6.2 Y34.0  \n", "G90  \n"]
    assert compare_real_jobs.classify_only("backend", resume, tool) == compare_real_jobs.TOOL_RESUME
    assert compare_real_jobs.classify_only("backend", resume + ["G1 X0 Y0  \n"], tool) is None
    assert compare_real_jobs.classify_only("backend", ["M3  \n", "G1 Z30.0 F1000.0  \n"], pause) == \
        compare_real_jobs.PAUSE_RESUME
    assert compare_real_jobs.classify_only("backend", ["M3  \n", "G0 Z-10  \n", "M5  \n"], pause) is None
    assert compare_real_jobs.classify_only("v094", ["G1 X1 Y1  \n"], pause) is None
    assert compare_real_jobs.classify_only("backend", ["G0 Z5.0  \n"], {}) is None
