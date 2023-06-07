package provider

import (
	"bytes"
	"log"
	"os"
	"os/exec"
	"strings"

	"github.com/malbertzard/repowatcher/model"
)

type RepoStorage interface {
    PullRepos() error
    DiffAllRepos() ([]string, error)
    DiffRepo(path string) (string, error)
}

type GitRepoStorage struct{
    Repos []model.Repo
}

func (g GitRepoStorage) PullRepos() error {
    // Iterate over the repos and pull the Git repositories
    for _, repo := range g.Repos {
        if err := g.pullRepo(repo.Path); err != nil {
            return err
        }
    }

    return nil
}

func (g GitRepoStorage) DiffAllRepos() ([]string, error) {
	var diffs []string

	for _, repo := range g.Repos {
		diff, err := g.diffRepo(repo.Path)
		if err != nil {
			return nil, err
		}
		if diff != "" {
			diffs = append(diffs, diff)
		}
	}

	return diffs, nil
}

func (g GitRepoStorage) DiffRepo(repo model.Repo) (string, error) {
    return g.diffRepo(repo.Path)
}

func (g GitRepoStorage) DiffRepoToCommit(repo model.Repo, lastCommit string) (string, error) {
    return g.diffRepo(repo.Path)
}

func (g GitRepoStorage) pullRepo(path string) error {
    cmd := exec.Command("git", "-C", path, "pull")
    cmd.Stdout = os.Stdout
    cmd.Stderr = os.Stderr
    return cmd.Run()
}

func (g GitRepoStorage) diffRepo(path string) (string, error) {
    cmd := exec.Command("git", "-C", path, "diff")
    output, err := cmd.Output()
    if err != nil {
        if exitError, ok := err.(*exec.ExitError); ok {
            // Git diff returns a non-zero exit code if there are no changes
            if exitError.ExitCode() == 1 {
                return "", nil
            }
        }
        return "", err
    }

    return strings.TrimSpace(string(output)), nil
}

//Helper functions

func pullRepo(path string) error {
    cmd := exec.Command("git", "pull")
    cmd.Dir = path
    err := cmd.Run()
    if err != nil {
        return err
    }
    return nil
}

func getLastCommitHash(path string) string {
    cmd := exec.Command("git", "rev-parse", "HEAD")
    cmd.Dir = path
    out, err := cmd.Output()
    if err != nil {
        log.Fatal(err)
    }
    return bytes.NewBuffer(out).String()
}

func getDiffFiles(LastCommit string, path string) []byte {
    cmd := exec.Command("git", "diff", LastCommit, "HEAD", "--name-only")
    cmd.Dir = path
    out, err := cmd.Output()
    if err != nil {
        log.Fatal(err)
    }
    return out
}

func getDiff(LastCommit string, path string) ([]byte, error) {
    cmd := exec.Command("git", "diff", LastCommit, "HEAD", "--color")
    cmd.Dir = path
    out, err := cmd.Output()
    if err != nil {
        return nil, err
    }
    return out, nil
}

func getDirsFromPath(dirPath string) ([]string, error) {
    dir, err := os.Open(dirPath)
    if err != nil {
        return nil, err
    }
    defer dir.Close()

    files, err := dir.ReadDir(-1) // -1 means read all entries
    if err != nil {
        return nil, err
    }

    var folders []string
    for _, file := range files {
        if file.IsDir() {
            folders = append(folders, file.Name())
        }
    }

    return folders, nil
}
