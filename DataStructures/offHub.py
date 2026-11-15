from gevent import get_hub


def runOffHub(function, *args):
    '''
    Runs CPU-bound work on a real OS thread and waits for it from the calling greenlet, so the hub keeps serving.
    `function` must not touch gevent queues, locks or sockets; exceptions re-raise in the caller.
    '''
    return get_hub().threadpool.apply(function, args)


class Collected(list):
    '''Stands in for a queue inside `runOffHub` work; replay its items onto the real queue from the hub.'''

    def put(self, *args):
        self.append(args)

    def replayOnto(self, queue):
        [queue.put(*args) for args in self]
