import os.path


application = defines.get("app")
appname = os.path.basename(application)

format = "UDZO"
files = [application]
symlinks = {"Applications": "/Applications"}

icon_locations = {
    appname: (140, 220),
    "Applications": (460, 220),
}

background = defines.get("background")
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False

window_rect = ((100, 100), (600, 400))
default_view = "icon-view"
arrange_by = None
grid_offset = (0, 0)
grid_spacing = 100
icon_size = 128
