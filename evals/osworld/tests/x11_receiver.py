"""Independent X11 receiver for the ignored public-Runner action gate.

This process only observes a Tk window. It never sends GUI input.
"""

import json
import os
import tkinter as tk

log = open(os.environ["AUV_OSWORLD_RECEIVER_LOG"], "a", encoding="utf-8", buffering=1)
root = tk.Tk()
root.title("AUV OSWorld action receiver")
root.geometry("1000x750+0+0")
text = tk.Text(root, wrap="none")
text.pack(fill="both", expand=True)


def record(kind, **fields):
    log.write(json.dumps({"kind": kind, **fields}, ensure_ascii=False) + "\n")


def mouse(event, kind):
    record(kind, button=event.num, x=event.x_root, y=event.y_root, state=event.state)


def motion(event):
    record("motion", x=event.x_root, y=event.y_root, state=event.state)


def key(event, kind):
    record(kind, keysym=event.keysym, char=event.char, state=event.state)


def modified(_event):
    if text.edit_modified():
        record("text", value=text.get("1.0", "end-1c"))
        text.edit_modified(False)


root.bind_all("<ButtonPress>", lambda event: mouse(event, "button_down"))
root.bind_all("<ButtonRelease>", lambda event: mouse(event, "button_up"))
root.bind_all("<Motion>", motion)
root.bind_all("<KeyPress>", lambda event: key(event, "key_down"))
root.bind_all("<KeyRelease>", lambda event: key(event, "key_up"))
text.bind("<<Modified>>", modified)
root.update()
text.focus_force()
record("ready")
root.mainloop()
