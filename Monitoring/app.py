import logging
from datetime import datetime
from nicegui import ui
import random
from scripts import mock_data, leos_func
import math

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

    # Initialize theme state
    is_dark_mode = False

    # Dark mode toggle button
    dark_mode_button = ui.button('Toggle Dark Mode').on_click(lambda: toggle_dark_mode())

    def toggle_dark_mode():
        nonlocal is_dark_mode
        is_dark_mode = not is_dark_mode  # Toggle the state
        # Use JavaScript to change the CSS
        ui.run_javascript(f"""
            document.body.style.backgroundColor = '{'black' if is_dark_mode else 'white'}';
            document.body.style.color = '{'white' if is_dark_mode else 'black'}';
        """)

    # Uncomment the following lines if you want to use logging handlers
    # update_hdl = LogElementHandler(update_log)
    # match_hdl = LogElementHandler(match_log)
    # sys_hdl = LogElementHandler(sys_log)

    # logger.addHandler(update_hdl)
    # logger.addHandler(match_hdl)
    # logger.addHandler(sys_hdl)

    # ui.context.client.on_disconnect(lambda: logger.removeHandler(update_hdl))
    # ui.context.client.on_disconnect(lambda: logger.removeHandler(match_hdl))
    # ui.context.client.on_disconnect(lambda: logger.removeHandler(sys_hdl))

    ui.timer(random.randint(1, 2), lambda: leos_func(random.randint(1, 5), update_log=update_log, matches_log=match_log, systems_log=sys_log))

    
@ui.page("/analytics")
def analytics():
    pass


     
ui.run()
