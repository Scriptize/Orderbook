import logging
from datetime import datetime
from nicegui import ui
import random
from scripts import mock_data, leos_func

logger = logging.getLogger()

class LogElementHandler(logging.Handler):
    """A logging handler that emits messages to a log element."""

    def __init__(self, element: ui.log, level: int = logging.NOTSET) -> None:
        self.element = element
        super().__init__(level)

    def emit(self, record: logging.LogRecord) -> None:
        try:
            msg = self.format(record)
            self.element.push(msg)
        except Exception:
            self.handleError(record)

@ui.page('/')
def page():
    update_log = ui.log().classes('w-full h-80')
    match_log = ui.log().classes('w-full h-80')
    sys_log = ui.log().classes('w-full h-80')

    # update_hdl = LogElementHandler(update_log)
    # match_hdl = LogElementHandler(match_log)
    # sys_hdl =  LogElementHandler(sys_log)

    # logger.addHandler(update_hdl)
    # logger.addHandler(match_hdl)
    # logger.addHandler(sys_hdl)

    # ui.context.client.on_disconnect(lambda: logger.removeHandler(update_hdl))
    # ui.context.client.on_disconnect(lambda: logger.removeHandler(match_hdl))
    # ui.context.client.on_disconnect(lambda: logger.removeHandler(sys_hdl))

    ui.timer(random.randint(1, 2), lambda: leos_func(random.randint(1, 5), update_log = update_log, matches_log = match_log, systems_log = sys_log))



ui.run()