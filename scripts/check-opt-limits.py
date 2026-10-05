import sys
import os

def read_file_safe(path):
    try:
        with open(path, 'r', encoding='utf-16') as f:
            return f.read()
    except:
        pass
    with open(path, 'r', encoding='utf-8', errors='ignore') as f:
        return f.read()

def check_limits(set_file, max_passes):
    if not os.path.isfile(set_file):
        print(f"Set file {set_file} not found.")
        return 0

    total_passes = 1
    has_opt = False

    content = read_file_safe(set_file)

    for line in content.splitlines():
        line = line.strip()
        if not line or line.startswith(';'):
            continue
        
        parts = line.split('=')
        if len(parts) < 2:
            continue
        
        val_str = parts[1]
        v_parts = val_str.split('||')
        if len(v_parts) == 5 and v_parts[4].upper() == 'Y':
            has_opt = True
            try:
                start = float(v_parts[1])
                step = float(v_parts[2])
                stop = float(v_parts[3])
                
                if step == 0:
                    steps = 1
                else:
                    steps = int(abs(stop - start) / abs(step)) + 1
                    if steps < 1:
                        steps = 1
                
                total_passes *= steps
            except ValueError:
                pass

    if not has_opt:
        print("No parameters enabled for optimization (missing 'Y' flag).")
        sys.exit(1)

    print(f"Estimated passes: {total_passes}")
    if max_passes > 0 and total_passes > max_passes:
        print(f"Error: Estimated passes ({total_passes}) exceed max limit ({max_passes}).")
        sys.exit(1)

    return total_passes

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: check-opt-limits.py <set_file> [max_passes]")
        sys.exit(2)
    
    set_file = sys.argv[1]
    max_passes = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    
    check_limits(set_file, max_passes)
