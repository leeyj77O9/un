import os
import platform
import sys

print(f"ENV|python|os={platform.platform()}|plat={sys.platform}|arch={platform.machine()}|cpu={os.cpu_count()}|64={sys.maxsize > 2**32}|impl={platform.python_implementation()}|version={platform.python_version()}")
