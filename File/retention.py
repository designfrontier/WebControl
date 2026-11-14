import glob
import os


def keepNewest(pattern, keep):
    """Deletes all but the `keep` most recently modified files matching the glob `pattern`."""
    paths = sorted(glob.glob(pattern), key=os.path.getmtime, reverse=True)
    for path in paths[keep:]:
        try:
            os.remove(path)
        except OSError as e:
            print("could not remove " + path + ": " + str(e))
