package main

import (
	"encoding/json"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"strconv"
)

type Input struct {
	File    string `json:"file"`
	Line    int    `json:"line"`
	Col     int    `json:"col"`
	Content string `json:"content"`
}

type Pos struct {
	Line int `json:"line"`
	Col  int `json:"col"`
}

type Range struct {
	Start Pos `json:"start"`
	End   Pos `json:"end"`
}

type UseEntry struct {
	Range    Range `json:"range"`
	Reassign bool  `json:"reassign"`
	Captured bool  `json:"captured"`
}

type Output struct {
	Name      string     `json:"name"`
	Decl      Range      `json:"decl"`
	Uses      []UseEntry `json:"uses"`
	IsPointer bool       `json:"is_pointer"`
}

type typeSwitchTarget struct {
	declIdent *ast.Ident
	objects   []types.Object
}

func main() {
	var in Input
	if err := json.NewDecoder(os.Stdin).Decode(&in); err != nil {
		encodeNil()
		return
	}
	out := resolve(in)
	enc := json.NewEncoder(os.Stdout)
	_ = enc.Encode(out)
}

func encodeNil() {
	enc := json.NewEncoder(os.Stdout)
	_ = enc.Encode((*Output)(nil))
}

func resolve(in Input) *Output {
	if in.File == "" {
		return nil
	}

	filePath := in.File
	if abs, err := filepath.Abs(filePath); err == nil {
		filePath = abs
	}

	fset := token.NewFileSet()
	file, files := parsePackageFiles(fset, filePath, in.Content)
	if file == nil || len(files) == 0 {
		return nil
	}

	info := &types.Info{
		Defs:       make(map[*ast.Ident]types.Object),
		Uses:       make(map[*ast.Ident]types.Object),
		Selections: make(map[*ast.SelectorExpr]*types.Selection),
		Types:      make(map[ast.Expr]types.TypeAndValue),
		Implicits:  make(map[ast.Node]types.Object),
	}
	config := &types.Config{
		Importer: importer.Default(),
		Error:    func(error) {},
	}
	pkgName := file.Name.Name
	_, _ = config.Check(pkgName, fset, files, info)

	parentMap := buildParentMap(file)
	ident, selMap := findIdentAtPosition(fset, file, in.Line, in.Col)
	if ident == nil {
		return nil
	}

	obj := info.Defs[ident]
	if obj == nil {
		obj = info.Uses[ident]
	}
	if obj == nil {
		if sel := selMap[ident]; sel != nil {
			if selInfo := info.Selections[sel]; selInfo != nil {
				obj = selInfo.Obj()
			}
		}
	}
	var tsTarget *typeSwitchTarget
	if obj == nil {
		tsTarget = resolveTypeSwitchTargetFromIdent(ident, info, parentMap)
		if tsTarget == nil {
			return nil
		}
	} else {
		switch obj.(type) {
		case *types.Func, *types.TypeName, *types.PkgName, *types.Builtin, *types.Label:
			return nil
		}
	}

	if tsTarget != nil {
		if tsTarget.declIdent == nil {
			return nil
		}
		decl := rangeForIdent(fset, tsTarget.declIdent)
		declFunc := enclosingFunc(tsTarget.declIdent, parentMap)
		uses := collectUsesForObjects(info, fset, tsTarget.objects, decl, declFunc, parentMap)
		isPointer := false
		for _, o := range tsTarget.objects {
			if isPointerType(o.Type()) {
				isPointer = true
				break
			}
		}
		return &Output{
			Name:      tsTarget.declIdent.Name,
			Decl:      decl,
			Uses:      uses,
			IsPointer: isPointer,
		}
	}

	declIdent := findDeclIdent(info, obj)
	if declIdent == nil {
		tsTarget = resolveTypeSwitchTargetFromObj(obj, info, parentMap)
		if tsTarget == nil || tsTarget.declIdent == nil {
			return nil
		}
		decl := rangeForIdent(fset, tsTarget.declIdent)
		declFunc := enclosingFunc(tsTarget.declIdent, parentMap)
		uses := collectUsesForObjects(info, fset, tsTarget.objects, decl, declFunc, parentMap)
		isPointer := false
		for _, o := range tsTarget.objects {
			if isPointerType(o.Type()) {
				isPointer = true
				break
			}
		}
		return &Output{
			Name:      tsTarget.declIdent.Name,
			Decl:      decl,
			Uses:      uses,
			IsPointer: isPointer,
		}
	}
	decl := rangeForIdent(fset, declIdent)
	declFunc := enclosingFunc(declIdent, parentMap)
	uses := collectUses(info, fset, obj, decl, declFunc, parentMap)

	return &Output{
		Name:      obj.Name(),
		Decl:      decl,
		Uses:      uses,
		IsPointer: isPointerType(obj.Type()),
	}
}

func parsePackageFiles(fset *token.FileSet, targetFile string, content string) (*ast.File, []*ast.File) {
	dir := filepath.Dir(targetFile)
	pkgs, err := parser.ParseDir(fset, dir, nil, parser.ParseComments)
	if err != nil {
		return parseSingleFile(fset, targetFile, content)
	}

	var targetPkg *ast.Package
	var targetAst *ast.File
	targetFile = filepath.Clean(targetFile)

	for _, pkg := range pkgs {
		for filename, f := range pkg.Files {
			if filepath.Clean(filename) == targetFile {
				targetPkg = pkg
				targetAst = f
			}
		}
	}

	if targetPkg == nil {
		return parseSingleFile(fset, targetFile, content)
	}

	// If we have overlay content for the target file, replace it
	if content != "" {
		if parsed, err := parser.ParseFile(fset, targetFile, content, parser.ParseComments); err == nil {
			targetPkg.Files[targetFile] = parsed
			targetAst = parsed
		}
	}

	files := make([]*ast.File, 0, len(targetPkg.Files))
	for _, f := range targetPkg.Files {
		files = append(files, f)
	}

	return targetAst, files
}

func parseSingleFile(fset *token.FileSet, targetFile string, content string) (*ast.File, []*ast.File) {
	var (
		file *ast.File
		err  error
	)
	if content != "" {
		file, err = parser.ParseFile(fset, targetFile, content, parser.ParseComments)
	} else {
		file, err = parser.ParseFile(fset, targetFile, nil, parser.ParseComments)
	}
	if err != nil || file == nil {
		return nil, nil
	}
	return file, []*ast.File{file}
}

func findIdentAtPosition(fset *token.FileSet, file *ast.File, line, col int) (*ast.Ident, map[*ast.Ident]*ast.SelectorExpr) {
	line++
	col++
	var best *ast.Ident
	bestSpan := 1 << 30
	selMap := make(map[*ast.Ident]*ast.SelectorExpr)

	ast.Inspect(file, func(n ast.Node) bool {
		switch node := n.(type) {
		case *ast.SelectorExpr:
			if node.Sel != nil {
				selMap[node.Sel] = node
			}
		case *ast.Ident:
			pos := fset.Position(node.Pos())
			end := fset.Position(node.End())
			if pos.Line != line {
				return true
			}
			if col < pos.Column || col > end.Column {
				return true
			}
			span := end.Column - pos.Column
			if span < bestSpan {
				bestSpan = span
				best = node
			}
		}
		return true
	})

	return best, selMap
}

func findDeclIdent(info *types.Info, obj types.Object) *ast.Ident {
	for ident, o := range info.Defs {
		if o == obj {
			return ident
		}
	}
	return nil
}

func collectUses(info *types.Info, fset *token.FileSet, obj types.Object, decl Range, declFunc ast.Node, parentMap map[ast.Node]ast.Node) []UseEntry {
	uses := make([]UseEntry, 0)
	seen := make(map[string]bool)
	objSet := map[types.Object]bool{obj: true}

	add := func(r Range, reassign bool, captured bool) {
		key := keyForRange(r)
		if seen[key] {
			return
		}
		if sameRange(r, decl) {
			return
		}
		seen[key] = true
		uses = append(uses, UseEntry{
			Range:    r,
			Reassign: reassign,
			Captured: captured,
		})
	}

	for ident, o := range info.Uses {
		if objSet[o] {
			r := rangeForIdent(fset, ident)
			add(r, isReassign(ident, info, parentMap), isCaptured(ident, obj, declFunc, parentMap))
		}
	}
	for sel, selInfo := range info.Selections {
		if selInfo != nil && objSet[selInfo.Obj()] {
			r := rangeForIdent(fset, sel.Sel)
			add(r, isReassign(sel.Sel, info, parentMap), isCaptured(sel.Sel, obj, declFunc, parentMap))
		}
	}

	return uses
}

func collectUsesForObjects(info *types.Info, fset *token.FileSet, objs []types.Object, decl Range, declFunc ast.Node, parentMap map[ast.Node]ast.Node) []UseEntry {
	objSet := make(map[types.Object]bool)
	for _, o := range objs {
		if o != nil {
			objSet[o] = true
		}
	}
	uses := make([]UseEntry, 0)
	seen := make(map[string]bool)

	add := func(r Range, reassign bool, captured bool) {
		key := keyForRange(r)
		if seen[key] {
			return
		}
		if sameRange(r, decl) {
			return
		}
		seen[key] = true
		uses = append(uses, UseEntry{
			Range:    r,
			Reassign: reassign,
			Captured: captured,
		})
	}

	for ident, o := range info.Uses {
		if objSet[o] {
			r := rangeForIdent(fset, ident)
			add(r, isReassign(ident, info, parentMap), isCaptured(ident, o, declFunc, parentMap))
		}
	}
	for sel, selInfo := range info.Selections {
		if selInfo != nil && objSet[selInfo.Obj()] {
			r := rangeForIdent(fset, sel.Sel)
			add(r, isReassign(sel.Sel, info, parentMap), isCaptured(sel.Sel, selInfo.Obj(), declFunc, parentMap))
		}
	}

	return uses
}

func resolveTypeSwitchTargetFromIdent(ident *ast.Ident, info *types.Info, parents map[ast.Node]ast.Node) *typeSwitchTarget {
	ts := enclosingTypeSwitch(ident, parents)
	if ts == nil {
		return nil
	}
	guard := typeSwitchGuardIdent(ts)
	if guard == nil || guard != ident {
		return nil
	}
	var objs []types.Object
	if ts.Body != nil {
		for _, stmt := range ts.Body.List {
			if cc, ok := stmt.(*ast.CaseClause); ok {
				if obj := info.Implicits[cc]; obj != nil {
					objs = append(objs, obj)
				}
			}
		}
	}
	if len(objs) == 0 {
		return nil
	}
	return &typeSwitchTarget{declIdent: guard, objects: objs}
}

func resolveTypeSwitchTargetFromObj(obj types.Object, info *types.Info, parents map[ast.Node]ast.Node) *typeSwitchTarget {
	if obj == nil {
		return nil
	}
	var ts *ast.TypeSwitchStmt
	for node, imp := range info.Implicits {
		if imp != obj {
			continue
		}
		if cc, ok := node.(*ast.CaseClause); ok {
			ts = enclosingTypeSwitch(cc, parents)
			break
		}
	}
	if ts == nil {
		return nil
	}
	guard := typeSwitchGuardIdent(ts)
	if guard == nil {
		return nil
	}
	var objs []types.Object
	if ts.Body != nil {
		for _, stmt := range ts.Body.List {
			if cc, ok := stmt.(*ast.CaseClause); ok {
				if o := info.Implicits[cc]; o != nil {
					objs = append(objs, o)
				}
			}
		}
	}
	if len(objs) == 0 {
		return nil
	}
	return &typeSwitchTarget{declIdent: guard, objects: objs}
}

func enclosingTypeSwitch(node ast.Node, parents map[ast.Node]ast.Node) *ast.TypeSwitchStmt {
	cur := node
	for cur != nil {
		if ts, ok := cur.(*ast.TypeSwitchStmt); ok {
			return ts
		}
		cur = parents[cur]
	}
	return nil
}

func typeSwitchGuardIdent(ts *ast.TypeSwitchStmt) *ast.Ident {
	if ts == nil || ts.Assign == nil {
		return nil
	}
	if as, ok := ts.Assign.(*ast.AssignStmt); ok {
		if len(as.Lhs) == 1 {
			if id, ok := as.Lhs[0].(*ast.Ident); ok {
				return id
			}
		}
	}
	return nil
}

func rangeForIdent(fset *token.FileSet, ident *ast.Ident) Range {
	start := fset.Position(ident.Pos())
	end := fset.Position(ident.End())
	return Range{
		Start: Pos{Line: start.Line - 1, Col: start.Column - 1},
		End:   Pos{Line: end.Line - 1, Col: end.Column - 1},
	}
}

func isPointerType(t types.Type) bool {
	if t == nil {
		return false
	}
	switch t.Underlying().(type) {
	case *types.Pointer, *types.Slice, *types.Map, *types.Chan, *types.Signature, *types.Interface:
		return true
	default:
		return false
	}
}

func sameRange(a, b Range) bool {
	return a.Start.Line == b.Start.Line &&
		a.Start.Col == b.Start.Col &&
		a.End.Line == b.End.Line &&
		a.End.Col == b.End.Col
}

func keyForRange(r Range) string {
	return strconv.Itoa(r.Start.Line) + ":" +
		strconv.Itoa(r.Start.Col) + ":" +
		strconv.Itoa(r.End.Line) + ":" +
		strconv.Itoa(r.End.Col)
}

func buildParentMap(root ast.Node) map[ast.Node]ast.Node {
	parents := make(map[ast.Node]ast.Node)
	var stack []ast.Node
	ast.Inspect(root, func(n ast.Node) bool {
		if n == nil {
			if len(stack) > 0 {
				stack = stack[:len(stack)-1]
			}
			return false
		}
		if len(stack) > 0 {
			parents[n] = stack[len(stack)-1]
		}
		stack = append(stack, n)
		return true
	})
	return parents
}

func enclosingFunc(node ast.Node, parents map[ast.Node]ast.Node) ast.Node {
	cur := node
	for cur != nil {
		switch cur.(type) {
		case *ast.FuncLit, *ast.FuncDecl:
			return cur
		}
		cur = parents[cur]
	}
	return nil
}

func isCaptured(ident *ast.Ident, obj types.Object, declFunc ast.Node, parents map[ast.Node]ast.Node) bool {
	useFunc := enclosingFunc(ident, parents)
	if useFunc == nil {
		return false
	}
	if _, ok := useFunc.(*ast.FuncLit); !ok {
		return false
	}
	if obj == nil || obj.Parent() == nil || obj.Pkg() == nil || obj.Parent() == obj.Pkg().Scope() {
		return false
	}
	if declFunc == nil {
		return false
	}
	return useFunc != declFunc
}

func isReassign(ident *ast.Ident, info *types.Info, parents map[ast.Node]ast.Node) bool {
	for n := ast.Node(ident); n != nil; n = parents[n] {
		parent := parents[n]
		switch stmt := parent.(type) {
		case *ast.AssignStmt:
			if !identIsAssignTargetInList(ident, stmt.Lhs) {
				return false
			}
			if stmt.Tok == token.DEFINE {
				return info.Defs[ident] == nil
			}
			return true
		case *ast.IncDecStmt:
			return identIsDirectTarget(ident, stmt.X)
		case *ast.RangeStmt:
			if !identIsDirectTarget(ident, stmt.Key) && !identIsDirectTarget(ident, stmt.Value) {
				return false
			}
			if stmt.Tok == token.DEFINE {
				return info.Defs[ident] == nil
			}
			return stmt.Tok == token.ASSIGN
		}
	}
	return false
}

func identIsAssignTargetInList(ident *ast.Ident, list []ast.Expr) bool {
	for _, expr := range list {
		if identIsDirectTarget(ident, expr) {
			return true
		}
	}
	return false
}

// identIsDirectTarget returns true only if ident is the direct assignment target:
// x = ..., obj.Field = ..., but NOT in index expressions or as part of larger expressions.
func identIsDirectTarget(ident *ast.Ident, expr ast.Expr) bool {
	switch e := expr.(type) {
	case *ast.Ident:
		return e == ident
	case *ast.SelectorExpr:
		return e.Sel == ident
	default:
		return false
	}
}

func identInExprList(ident *ast.Ident, list []ast.Expr) bool {
	for _, expr := range list {
		if identInExpr(ident, expr) {
			return true
		}
	}
	return false
}

func identInExpr(ident *ast.Ident, expr ast.Expr) bool {
	if expr == nil {
		return false
	}
	switch e := expr.(type) {
	case *ast.Ident:
		return e == ident
	case *ast.SelectorExpr:
		return identInExpr(ident, e.X) || e.Sel == ident
	case *ast.IndexExpr:
		return identInExpr(ident, e.X) || identInExpr(ident, e.Index)
	case *ast.StarExpr:
		return identInExpr(ident, e.X)
	case *ast.UnaryExpr:
		return identInExpr(ident, e.X)
	case *ast.BinaryExpr:
		return identInExpr(ident, e.X) || identInExpr(ident, e.Y)
	case *ast.CallExpr:
		if identInExpr(ident, e.Fun) {
			return true
		}
		for _, arg := range e.Args {
			if identInExpr(ident, arg) {
				return true
			}
		}
		return false
	case *ast.ParenExpr:
		return identInExpr(ident, e.X)
	default:
		return false
	}
}
