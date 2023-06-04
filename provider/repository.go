package provider

import (
	"bytes"
	"log"
	"os"
	"os/exec"
)

//TODO Git Provider

type RepoStorage interface {
	PullRepos() error
	DiffAllRepos() ([]string, error)
	DiffRepo(path string) (string, error)
}

type GitRepoStorage struct {
	basePath string
}

func (g *GitRepoStorage) PullRepos() error {
	paths, err := getDirsFromPath(g.basePath)
	if err != nil {
		return err
	}

	for _, path := range paths {
        err = pullRepo(g.basePath + path)
		if err != nil {
			return err
		}
	}
	return nil
}

func (g *GitRepoStorage) DiffAllRepos() ([]string, error) {
	paths, err := getDirsFromPath(g.basePath)
	if err != nil {
		return nil, err
	}

	var diffList []string
	var bytes []byte
	for _, path := range paths {
		repoPath := g.basePath + path
		lastCommit := getLastCommitHash(repoPath)

		bytes, err = getDiff(lastCommit, repoPath)

		if err != nil {
			return nil, err
		}
		diffList = append(diffList, string(bytes))
		if err != nil {
			return nil, err
		}
	}
	return diffList, nil
}

func (g *GitRepoStorage) DiffRepo(path string) (string, error) {
	repoPath := g.basePath + path
	lastCommit := getLastCommitHash(repoPath)

	bytes, err := getDiff(lastCommit, repoPath)

	if err != nil {
		return "", err
	}
	return string(bytes), nil
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
