#!/usr/bin/env python3
import sys
import re
import os

def is_readonly(func_name):
    """Returns True if the function is a read-only getter or view query."""
    return (
        func_name.startswith("get_")
        or func_name.startswith("preview_")
        or func_name.startswith("convert_")
        or func_name == "is_paused"
    )

def parse_security_md(filepath):
    with open(filepath, "r", encoding="utf-8") as f:
        content = f.read()

    in_table = False
    table_lines = []
    for line in content.splitlines():
        stripped = line.strip()
        if stripped.startswith("| Function"):
            in_table = True
            continue
        if in_table:
            if stripped.startswith("|---") or stripped.startswith("| ---"):
                continue
            if stripped.startswith("|") and not stripped.startswith("| Function"):
                table_lines.append(stripped)
            else:
                break

    functions = {}
    for line in table_lines:
        cells = [c.strip() for c in line.split("|")]
        cells = [c for c in cells if c]
        if len(cells) < 5:
            continue
        func_name = cells[0].strip()
        
        # Skip read-only getters; they are documented in Read-Only / View Functions
        if is_readonly(func_name):
            continue

        owner = cells[1].strip() in ("yes", "✅")
        agent = cells[2].strip() in ("yes", "✅")
        user = cells[3].strip() in ("yes", "✅")
        anyone_raw = cells[4].strip()
        
        if anyone_raw == "pending owner":
            access = "pending-owner"
        elif anyone_raw == "anyone":
            access = "anyone"
        elif owner:
            access = "owner"
        elif agent:
            access = "agent"
        elif user:
            access = "user"
        else:
            access = "unknown"
            
        functions[func_name] = access
    return functions

def parse_lib_rs(filepath):
    with open(filepath, "r", encoding="utf-8") as f:
        content = f.read()
    
    parts = content.split("pub fn ")[1:]
    functions = {}
    
    for part in parts:
        match = re.match(r"^([a-zA-Z0-9_]+)\s*\(", part)
        if not match:
            continue
        func_name = match.group(1)
        
        if func_name == "initialize" or is_readonly(func_name):
            continue
        
        brace_count = 0
        in_body = False
        body_end = 0
        for i, char in enumerate(part):
            if char == '{':
                in_body = True
                brace_count += 1
            elif char == '}':
                brace_count -= 1
                if in_body and brace_count == 0:
                    body_end = i
                    break
        
        body = part[:body_end+1] if body_end > 0 else part
        
        access = "anyone"
        
        # First check explicit require_auth
        auth_match = re.search(r"([a-zA-Z0-9_]+)\.require_auth\(\)", body)
        if auth_match:
            var = auth_match.group(1)
            if var == "owner":
                access = "owner"
            elif var == "agent":
                access = "agent"
            elif var == "user":
                access = "user"
            elif var == "new_owner":
                access = "pending-owner"
        elif "require_auth" in body:
            if "owner" in body.lower():
                access = "owner"
            elif "user" in body.lower():
                access = "user"
            elif "agent" in body.lower():
                access = "agent"
        else:
            # Fallback heuristics for functions without direct require_auth
            if "Self::require_is_owner" in body or "OnlyOwner" in body or "VaultError::CallerIsNotOwner" in body:
                access = "owner"
            elif "Self::require_is_agent" in body or "OnlyAgent" in body or "VaultError::OnlyAgentCanUpdateTotalAssets" in body:
                access = "agent"
            elif "Self::require_is_pending_owner" in body or "CallerIsNotPendingOwner" in body:
                access = "pending-owner"
            elif "emergency_harvest" in func_name and "owner" in body.lower():
                access = "owner"
            
        functions[func_name] = access

    return functions

def main():
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    security_md_path = os.path.join(repo_root, "SECURITY.md")
    lib_rs_path = os.path.join(repo_root, "neurowealth-vault", "contracts", "vault", "src", "lib.rs")
    
    sec_funcs = parse_security_md(security_md_path)
    lib_funcs = parse_lib_rs(lib_rs_path)
    
    errors = False
    
    for func, sec_access in sec_funcs.items():
        if is_readonly(func):
            continue
        if func not in lib_funcs:
            print(f"FAIL: Function {func} is in SECURITY.md but not found as a state-changing function in lib.rs.")
            errors = True
        else:
            lib_access = lib_funcs[func]
            if lib_access != sec_access:
                print(f"FAIL: Mismatch for {func}: SECURITY.md says '{sec_access}', lib.rs code says '{lib_access}'")
                errors = True

    if not errors:
        print("PASS: Auth matrix matches between SECURITY.md and lib.rs")
        sys.exit(0)
    else:
        sys.exit(1)

if __name__ == "__main__":
    main()

