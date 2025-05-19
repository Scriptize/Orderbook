import logging
from datetime import datetime
from nicegui import ui
import random

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
    update_log = ui.log(max_lines=10).classes('w-full')
    match_log = ui.log(max_lines=10).classes('w-full')
    sys_log = ui.log(max_lines=10).classes('w-full')

    update_hdl = LogElementHandler(update_log)
    match_hdl = LogElementHandler(match_log)
    sys_hdl =  LogElementHandler(sys_log)

    logger.addHandler(update_hdl)
    logger.addHandler(match_hdl)
    logger.addHandler(sys_hdl)

    ui.context.client.on_disconnect(lambda: logger.removeHandler(update_hdl))
    ui.context.client.on_disconnect(lambda: logger.removeHandler(match_hdl))
    ui.context.client.on_disconnect(lambda: logger.removeHandler(sys_hdl))

    # ui.timer(random.randint(7),)



ui.run()