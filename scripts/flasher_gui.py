import tkinter as tk
from tkinter import ttk, scrolledtext, messagebox
import subprocess
import threading
import os
import sys
import time

try:
    import customtkinter as ctk
except ImportError:
    ctk = None

try:
    import serial
except ImportError:
    serial = None

# Cyberpunk Color Palette
COLORS = {
    "bg": "#0a0a12",
    "fg": "#e0e0ff",
    "accent1": "#00f2ff",  # Cyan
    "accent2": "#ff00ff",  # Magenta
    "accent3": "#7000ff",  # Purple
    "success": "#39ff14",  # Neon Green
    "warning": "#ffff00",  # Neon Yellow
    "error": "#ff3131",    # Neon Red
    "frame": "#161625"
}

class AutoMonitor:
    def __init__(self, gui):
        self.gui = gui
        self.running = True
        self.ser = None
        self.port = None
        self.active = False
        self.thread = threading.Thread(target=self._monitor_loop, daemon=True)
        self.thread.start()

    def _monitor_loop(self):
        while self.running:
            if not self.active or not self.gui.combo_port.get():
                time.sleep(1)
                continue

            target_port = self.gui.combo_port.get()
            
            if self.ser and self.port == target_port:
                try:
                    if self.ser.in_waiting:
                        line = self.ser.readline().decode('utf-8', errors='ignore')
                        self.gui.log_monitor(line.strip())
                    time.sleep(0.01)
                except Exception:
                    self.close()
            else:
                self.close()
                self.port = target_port
                try:
                    self.ser = serial.Serial(self.port, 115200, timeout=0.1)
                    self.gui.log_monitor(f" Connected to {self.port} at 115200", is_info=True)
                except Exception:
                    time.sleep(1)

    def close(self):
        if self.ser:
            try:
                self.ser.close()
            except:
                pass
            self.ser = None
            if self.port:
                self.gui.log_monitor(f" Disconnected from {self.port}", is_info=True)
        self.port = None

    def send_command(self, cmd):
        if self.ser and self.ser.is_open:
            try:
                self.ser.write(f"{cmd}\n".encode())
                return True
            except:
                return False
        return False

class FlasherGUI:
    def __init__(self, root):
        self.root = root
        self.root.title("CARDPUTER COMMAND CENTER v3")
        self.root.geometry("1200x900")
        self.root.configure(bg=COLORS["bg"])

        if ctk:
            ctk.set_appearance_mode("dark")
            ctk.set_default_color_theme("blue")

        self.espflash_path = self.find_espflash()
        self.setup_ui()
        
        if serial:
            self.monitor = AutoMonitor(self)
        else:
            self.monitor = None
            
        self.update_ports()

    def find_espflash(self):
        local_path = os.path.abspath("temp_espflash/espflash.exe")
        if os.path.exists(local_path):
            return local_path
        return "espflash"

    def setup_ui(self):
        # Header
        header = ctk.CTkFrame(self.root, fg_color="transparent") if ctk else tk.Frame(self.root, bg=COLORS["bg"])
        header.pack(fill=tk.X, padx=20, pady=10)

        title_font = ("Orbitron", 32, "bold") if ctk else ("Arial", 24, "bold")
        lbl_title = ctk.CTkLabel(header, text="CARDPUTER COMMAND CENTER", font=title_font, text_color=COLORS["accent1"]) if ctk else \
                    tk.Label(header, text="CARDPUTER COMMAND CENTER", font=title_font, bg=COLORS["bg"], fg=COLORS["accent1"])
        lbl_title.pack(side=tk.LEFT)

        # Main Layout: Right (Logs) + Left (Controls)
        content = ctk.CTkFrame(self.root, fg_color="transparent") if ctk else tk.Frame(self.root, bg=COLORS["bg"])
        content.pack(fill=tk.BOTH, expand=True, padx=20, pady=10)

        # Left Panel (Controls)
        left_panel = ctk.CTkFrame(content, fg_color=COLORS["frame"], width=400) if ctk else tk.Frame(content, bg=COLORS["frame"], width=400)
        left_panel.pack(side=tk.LEFT, fill=tk.BOTH, padx=(0, 10))

        # --- Section: Connection ---
        conn_frame = self.create_section(left_panel, "CONNECTION")
        self.combo_port = ctk.CTkComboBox(conn_frame, values=["Checking..."], width=200) if ctk else ttk.Combobox(conn_frame, width=15)
        self.combo_port.pack(side=tk.LEFT, padx=10, pady=10)
        btn_refresh = ctk.CTkButton(conn_frame, text="SCAN", width=80, fg_color=COLORS["accent3"], command=self.update_ports) if ctk else \
                      tk.Button(conn_frame, text="SCAN", command=self.update_ports)
        btn_refresh.pack(side=tk.LEFT, padx=5)

        # --- Section: Target Selection ---
        target_frame = self.create_section(left_panel, "DEPLOYMENT")
        targets = ["loader", "graphics", "python_runner", "rink", "sound", "espnow_remote"]
        self.combo_target = ctk.CTkComboBox(target_frame, values=targets, width=280) if ctk else ttk.Combobox(target_frame, values=targets, width=25)
        self.combo_target.pack(padx=10, pady=10)
        self.combo_target.set("loader")

        btn_build = ctk.CTkButton(target_frame, text="COMPILE FIRMWARE", fg_color=COLORS["accent2"], command=self.on_build) if ctk else \
                    tk.Button(target_frame, text="COMPILE", command=self.on_build)
        btn_build.pack(fill=tk.X, padx=10, pady=5)

        btn_flash = ctk.CTkButton(target_frame, text="FLASH TO DEVICE", fg_color=COLORS["success"], text_color="#000", command=self.on_flash) if ctk else \
                    tk.Button(target_frame, text="FLASH", command=self.on_flash)
        btn_flash.pack(fill=tk.X, padx=10, pady=5)

        # --- Section: Device Control ---
        ctrl_frame = self.create_section(left_panel, "HARDWARE CONTROL")
        btn_recovery = ctk.CTkButton(ctrl_frame, text="REBOOT TO RECOVERY", fg_color=COLORS["error"], command=lambda: self.send_serial("RECOVERY")) if ctk else \
                       tk.Button(ctrl_frame, text="RECOVERY", command=lambda: self.send_serial("RECOVERY"))
        btn_recovery.pack(fill=tk.X, padx=10, pady=5)

        btn_reboot = ctk.CTkButton(ctrl_frame, text="SYSTEM RESTART", command=lambda: self.send_serial("REBOOT")) if ctk else \
                     tk.Button(ctrl_frame, text="REBOOT", command=lambda: self.send_serial("REBOOT"))
        btn_reboot.pack(fill=tk.X, padx=10, pady=5)

        btn_erase = ctk.CTkButton(ctrl_frame, text="ERASE ALL FLASH", fg_color="#333", text_color=COLORS["error"], command=self.on_erase_flash) if ctk else \
                    tk.Button(ctrl_frame, text="ERASE FLASH", command=self.on_erase_flash)
        btn_erase.pack(fill=tk.X, padx=10, pady=5)

        self.monitor_active = tk.BooleanVar(value=True)
        chk_monitor = ctk.CTkCheckBox(ctrl_frame, text="LIVE MONITOR ACTIVE", variable=self.monitor_active, command=self.on_toggle_monitor) if ctk else \
                      tk.Checkbutton(ctrl_frame, text="Monitor", variable=self.monitor_active)
        chk_monitor.pack(pady=10)

        # Right Panel (Logs)
        right_panel = ctk.CTkFrame(content, fg_color="transparent") if ctk else tk.Frame(content, bg=COLORS["bg"])
        right_panel.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)

        # Live Monitor Area
        monitor_header = ctk.CTkFrame(right_panel, fg_color="transparent") if ctk else tk.Frame(right_panel, bg=COLORS["bg"])
        monitor_header.pack(fill=tk.X)
        
        monitor_label = ctk.CTkLabel(monitor_header, text="LIVE DEVICE STREAM", font=("Arial", 14, "bold"), text_color=COLORS["accent1"]) if ctk else \
                        tk.Label(monitor_header, text="LIVE STREAM", bg=COLORS["bg"], fg=COLORS["accent1"])
        monitor_label.pack(side=tk.LEFT, anchor=tk.W)

        btn_copy_monitor = ctk.CTkButton(monitor_header, text="COPY", width=60, height=20, font=("Arial", 10), command=lambda: self.on_copy_text(self.monitor_area)) if ctk else \
                           tk.Button(monitor_header, text="COPY", command=lambda: self.on_copy_text(self.monitor_area))
        btn_copy_monitor.pack(side=tk.RIGHT, padx=5)
        
        btn_clear_monitor = ctk.CTkButton(monitor_header, text="CLEAR", width=60, height=20, font=("Arial", 10), fg_color="#333", command=lambda: self.on_clear_text(self.monitor_area)) if ctk else \
                            tk.Button(monitor_header, text="CLEAR", command=lambda: self.on_clear_text(self.monitor_area))
        btn_clear_monitor.pack(side=tk.RIGHT, padx=5)

        self.monitor_area = scrolledtext.ScrolledText(right_panel, height=30, bg="#000", fg=COLORS["success"], font=("Consolas", 10))
        self.monitor_area.pack(fill=tk.BOTH, expand=True, pady=(0, 10))

        # System Log Area
        system_header = ctk.CTkFrame(right_panel, fg_color="transparent") if ctk else tk.Frame(right_panel, bg=COLORS["bg"])
        system_header.pack(fill=tk.X)

        system_label = ctk.CTkLabel(system_header, text="SYSTEM DIAGNOSTICS", font=("Arial", 14, "bold"), text_color=COLORS["accent2"]) if ctk else \
                       tk.Label(system_header, text="SYSTEM LOG", bg=COLORS["bg"], fg=COLORS["accent2"])
        system_label.pack(side=tk.LEFT, anchor=tk.W)

        btn_copy_log = ctk.CTkButton(system_header, text="COPY", width=60, height=20, font=("Arial", 10), command=lambda: self.on_copy_text(self.log_area)) if ctk else \
                       tk.Button(system_header, text="COPY", command=lambda: self.on_copy_text(self.log_area))
        btn_copy_log.pack(side=tk.RIGHT, padx=5)

        btn_clear_log = ctk.CTkButton(system_header, text="CLEAR", width=60, height=20, font=("Arial", 10), fg_color="#333", command=lambda: self.on_clear_text(self.log_area)) if ctk else \
                        tk.Button(system_header, text="CLEAR", command=lambda: self.on_clear_text(self.log_area))
        btn_clear_log.pack(side=tk.RIGHT, padx=5)

        self.log_area = scrolledtext.ScrolledText(right_panel, height=12, bg="#050505", fg=COLORS["fg"], font=("Consolas", 10))
        self.log_area.pack(fill=tk.BOTH)

    def create_section(self, parent, title):
        frame = ctk.CTkFrame(parent, fg_color="transparent") if ctk else tk.Frame(parent, bg=COLORS["frame"])
        frame.pack(fill=tk.X, padx=10, pady=10)
        lbl = ctk.CTkLabel(frame, text=title, font=("Arial", 12, "bold"), text_color="#666") if ctk else \
              tk.Label(frame, text=title, font=("Arial", 10, "bold"), fg="#666", bg=COLORS["frame"])
        lbl.pack(anchor=tk.W, padx=5)
        return frame

    def log(self, text, color=None):
        self.log_area.insert(tk.END, text + "\n")
        self.log_area.see(tk.END)

    def on_copy_text(self, widget):
        content = widget.get("1.0", tk.END)
        self.root.clipboard_clear()
        self.root.clipboard_append(content)
        self.log("[INFO] Content copied to clipboard.")

    def on_clear_text(self, widget):
        widget.delete("1.0", tk.END)
        self.log("[INFO] Panel cleared.")

    def log_monitor(self, text, is_info=False):
        if not text: return
        self.monitor_area.insert(tk.END, ("> " if is_info else "") + text + "\n")
        self.monitor_area.see(tk.END)
        # Limit buffer
        if int(self.monitor_area.index('end-1c').split('.')[0]) > 1000:
            self.monitor_area.delete('1.0', '200.0')

    def update_ports(self):
        try:
            cmd = 'powershell -NoProfile -Command "Get-CimInstance Win32_SerialPort | Select-Object -ExpandProperty DeviceID"'
            result = subprocess.run(cmd, capture_output=True, text=True, shell=True)
            ports = result.stdout.strip().split('\n')
            ports = [p.strip() for p in ports if p.strip()]
            if not ports: ports = ["COM1"]
            
            if ctk:
                self.combo_port.configure(values=ports)
                self.combo_port.set(ports[0])
            else:
                self.combo_port['values'] = ports
                self.combo_port.set(ports[0])
            self.log(f"Found ports: {ports}")
        except Exception as e:
            self.log(f"Port scan error: {e}")

    def on_toggle_monitor(self):
        if self.monitor:
            self.monitor.active = self.monitor_active.get()
            if not self.monitor.active: self.monitor.close()

    def send_serial(self, cmd):
        if not self.monitor: return
        self.log(f"Sending command: {cmd}")
        # Temporarily steal focus from monitor if needed, but AutoMonitor handles it
        if not self.monitor.send_command(cmd):
            self.log("Failed to send command. Device not connected or monitor inactive.", COLORS["error"])

    def on_build(self):
        target = self.combo_target.get()
        threading.Thread(target=self._build_task, args=(target,), daemon=True).start()

    def _build_task(self, target):
        self.log(f"\n[BUILD] Using Persistent Docker Container for: {target}")
        
        # 1. Ensure container is running
        check = subprocess.run("docker compose ps -q builder", shell=True, capture_output=True, text=True)
        if not check.stdout.strip():
             self.log("[INFO] Starting Docker container (this may take a moment)...")
             if self.run_command(["docker", "compose", "up", "-d", "--build"]) != 0:
                 self.log("[ERROR] Failed to start container")
                 return

        # 2. Run build script inside container
        self.log("[INFO] Running incremental build inside container...")
        self.run_command(["docker", "compose", "exec", "-T", "builder", "bash", "scripts/internal_build.sh"])

    def on_flash(self):
        target = self.combo_target.get()
        threading.Thread(target=self._flash_task, args=(target,), daemon=True).start()

    def _flash_task(self, target):
        # Pause monitor during flash
        was_active = self.monitor_active.get()
        if self.monitor: 
            self.monitor_active.set(False)
            self.monitor.active = False
            self.monitor.close()
            time.sleep(0.5)

        self.log(f"\n[FLASH] Deploying {target}...")
        elf = self.find_elf(target)
        if not elf:
            self.log("Error: ELF file not found. Build first.")
            return

        port = self.combo_port.get()
        # Use espflash to flash. Since we handle monitor ourselves, we don't use --monitor here.
        self.run_command([self.espflash_path, "flash", "--chip", "esp32s3", "--port", port, "--baud", "921600", elf])
        
        # Resume monitor
        if was_active:
            time.sleep(1)
            self.monitor_active.set(True)
            if self.monitor: 
                self.monitor.active = True

    def on_erase_flash(self):
        if not messagebox.askyesno("ERASE FLASH", "This will WIPE the device. Are you sure?"):
            return
        threading.Thread(target=self._erase_task, daemon=True).start()

    def _erase_task(self):
        # Pause monitor
        was_active = self.monitor_active.get()
        if self.monitor: 
            self.monitor_active.set(False)
            self.monitor.active = False
            self.monitor.close()
            time.sleep(0.5)

        self.log("\n[ERASE] Wiping flash...")
        port = self.combo_port.get()
        self.run_command([self.espflash_path, "erase-flash", "--chip", "esp32s3", "--port", port])

        if was_active:
            time.sleep(1)
            self.monitor_active.set(True)
            if self.monitor: 
                self.monitor.active = True

    def find_elf(self, bin_name):
        # We now copy the binary to target/loader/loader (or just target/bin_name if generalized)
        # Check specific exfiltrated path first
        paths = [
            f"target/{bin_name}/{bin_name}", # New structure from internal_build.sh
            f"target/{bin_name}",            # Fallback
            f"target/xtensa-esp32s3-espidf/release/{bin_name}" # Old structure
        ]
        for p in paths:
            if os.path.exists(p): return p
        return None

    def run_command(self, cmd_list):
        self.log(f"RUN: {' '.join(cmd_list)}")
        process = subprocess.Popen(cmd_list, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, encoding='utf-8', errors='replace', shell=True)
        for line in process.stdout:
            self.log(line.strip())
        process.wait()
        return process.returncode

if __name__ == "__main__":
    if ctk:
        root = ctk.CTk()
    else:
        root = tk.Tk()
    app = FlasherGUI(root)
    root.mainloop()
